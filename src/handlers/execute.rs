use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures::Stream;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{FlowError, Result},
    middleware::FlowUserExt,
    models::execution::NodeLog,
    models::workflow::WorkflowDefinition,
    nodes::trait_::{ExecutionContext, NodeContext},
    runtime::{executor::Executor, resolver},
    state::AppState,
};

/// POST /workflows/:id/execute — exécution manuelle immédiate (hors file).
/// Retourne l'`execution_id` à streamer.
pub async fn execute(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>> {
    let row = sqlx::query_as::<_, (Option<Uuid>,)>(
        "SELECT file_id FROM flow.workflows WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| FlowError::NotFound("Workflow introuvable".into()))?;

    let def_value = match row.0 {
        Some(fid) => crate::services::content_files::read_definition(&state, user.id, fid).await?,
        None => crate::services::content_files::empty_definition(),
    };
    let definition = WorkflowDefinition::from_value(&def_value);
    let trigger_data = body.map(|b| b.0).unwrap_or(json!({}));

    let execution_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO flow.executions
            (workflow_id, owner_id, status, trigger_source, trigger_data, nodes_total)
           VALUES ($1,$2,'running','manual',$3,$4) RETURNING id"#,
    )
    .bind(id)
    .bind(user.id)
    .bind(&trigger_data)
    .bind(definition.nodes.len() as i32)
    .fetch_one(&state.db)
    .await?;

    // Exécution en tâche de fond (détachée de la requête) : elle se poursuit côté
    // serveur même si l'utilisateur quitte l'application. Le fichier .kbflw est
    // protégé pendant toute la durée de l'exécution (suppression bloquée).
    let st = state.clone();
    let owner = user.id;
    let file_id = row.0;
    tokio::spawn(async move {
        if let Some(fid) = file_id {
            let _ = st.files_client.set_file_protected(owner, fid, true).await;
        }
        let executor = Executor {
            db:           st.db.clone(),
            registry:     st.registry.clone(),
            proxy:        st.proxy.clone(),
            settings:     st.settings.clone(),
            files_client: st.files_client.clone(),
        };
        let outcome = executor.run(execution_id, owner, id, &definition, trigger_data, 1).await;
        if let Some(fid) = file_id {
            let _ = st.files_client.set_file_protected(owner, fid, false).await;
        }
        // Déclencheur d'erreur sur un run manuel échoué.
        if outcome.status == "error" {
            let wf_name = sqlx::query_scalar::<_, String>("SELECT name FROM flow.workflows WHERE id = $1")
                .bind(id).fetch_optional(&st.db).await.ok().flatten().unwrap_or_default();
            let msg = outcome.error_message.clone().unwrap_or_default();
            crate::runtime::scheduler::dispatch_error_workflows(&st, id, &wf_name, owner, execution_id, &msg).await;
        }
    });

    Ok(Json(json!({ "execution_id": execution_id })))
}

/// GET /executions/:id/stream — SSE des logs de nœuds en temps réel (polling DB).
pub async fn stream(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        let mut seen = 0usize;
        let mut ticks = 0u32;
        loop {
            // Vérifier l'appartenance + statut.
            let exec = sqlx::query_as::<_, (String, Uuid)>(
                "SELECT status, owner_id FROM flow.executions WHERE id = $1",
            )
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

            let Some((status, owner_id)) = exec else {
                yield Ok(Event::default().event("error").data("introuvable"));
                break;
            };
            if owner_id != user.id {
                yield Ok(Event::default().event("error").data("interdit"));
                break;
            }

            let logs = sqlx::query_as::<_, NodeLog>(
                "SELECT * FROM flow.node_logs WHERE execution_id = $1 ORDER BY executed_at ASC",
            )
            .bind(id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

            for log in logs.iter().skip(seen) {
                if let Ok(data) = serde_json::to_string(log) {
                    yield Ok(Event::default().event("node").data(data));
                }
            }
            seen = logs.len();

            if status != "running" {
                yield Ok(Event::default().event("done").data(status));
                break;
            }

            ticks += 1;
            if ticks > 1800 { // garde-fou ~6 min
                yield Ok(Event::default().event("done").data("timeout"));
                break;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[derive(Deserialize)]
pub struct TestNodeBody {
    #[serde(default)]
    pub input_data: Value,
}

/// POST /workflows/:id/nodes/:node_id/test — teste un nœud isolément.
pub async fn test_node(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path((id, node_id)): Path<(Uuid, String)>,
    Json(body): Json<TestNodeBody>,
) -> Result<Json<Value>> {
    let row = sqlx::query_as::<_, (Option<Uuid>,)>(
        "SELECT file_id FROM flow.workflows WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| FlowError::NotFound("Workflow introuvable".into()))?;

    let def_value = match row.0 {
        Some(fid) => crate::services::content_files::read_definition(&state, user.id, fid).await?,
        None => crate::services::content_files::empty_definition(),
    };
    let definition = WorkflowDefinition::from_value(&def_value);
    let node = definition
        .nodes
        .iter()
        .find(|n| n.id == node_id)
        .ok_or_else(|| FlowError::NotFound("Nœud introuvable".into()))?;

    let executor = state
        .registry
        .get(&node.node_type)
        .ok_or_else(|| FlowError::Validation(format!("Type de nœud inconnu : {}", node.node_type)))?;

    let input = body.input_data;
    let mut ctx_map = serde_json::Map::new();
    ctx_map.insert("trigger".into(), input.clone());
    ctx_map.insert("nodes".into(), json!({}));
    ctx_map.insert("input".into(), input.clone());
    ctx_map.insert("json".into(), input.clone());
    ctx_map.insert("$json".into(), input.clone());
    ctx_map.insert("$input".into(), input.clone());
    ctx_map.insert("$workflow".into(), json!({ "id": id.to_string() }));
    ctx_map.insert("$execution".into(), json!({ "id": Uuid::nil().to_string(), "mode": "test" }));
    crate::runtime::expr::with_now(&mut ctx_map);
    let full = Value::Object(ctx_map);
    let exec_ctx = ExecutionContext {
        execution_id:    Uuid::nil(),
        workflow_id:     id,
        owner_id:        user.id,
        current_node_id: node.id.clone(),
        attempt:         1,
        input:           input.clone(),
        full: full.clone(),
    };
    let node_ctx = NodeContext {
        proxy: &state.proxy, user_id: user.id, db: &state.db, settings: &state.settings,
        registry: &state.registry, files_client: &state.files_client, depth: 0,
    };

    let mut resolved = resolver::resolve_value(&node.config, &full);
    crate::services::credentials::inject_into_config(
        &state.registry, &state.db, &state.settings.core.internal_secret,
        user.id, &node.node_type, &mut resolved,
    ).await;
    // Agent IA : rassembler les sous-nœuds branchés pour que « Tester » fonctionne.
    let subs = state.registry.sub_inputs(&node.node_type);
    if !subs.is_empty() {
        let by_id: std::collections::HashMap<&str, &crate::models::workflow::WorkflowNode> =
            definition.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let mut sub_map = serde_json::Map::new();
        for si in &subs {
            let mut items = Vec::new();
            for e in definition.edges.iter().filter(|e| e.target == node.id && e.target_port.as_deref() == Some(si.id.as_str())) {
                if let Some(src) = by_id.get(e.source.as_str()) {
                    let mut sc = resolver::resolve_value(&src.config, &full);
                    crate::services::credentials::inject_into_config(
                        &state.registry, &state.db, &state.settings.core.internal_secret,
                        user.id, &src.node_type, &mut sc,
                    ).await;
                    items.push(json!({ "type": src.node_type, "name": src.name, "config": sc }));
                }
            }
            sub_map.insert(si.id.clone(), Value::Array(items));
        }
        if let Some(obj) = resolved.as_object_mut() { obj.insert("__sub".into(), Value::Object(sub_map)); }
    }
    match executor.execute(resolved, &exec_ctx, &node_ctx).await {
        Ok(out) => Ok(Json(json!({ "success": true, "output": out.data }))),
        Err(e)  => Ok(Json(json!({ "success": false, "error": e.to_string() }))),
    }
}
