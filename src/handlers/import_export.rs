use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{FlowError, Result},
    middleware::FlowUserExt,
    models::workflow::{Workflow, WorkflowDefinition},
    services::content_files as cf,
    state::AppState,
};

/// GET /workflows/:id/export — export JSON (format Kubuno, compatible n8n-like).
pub async fn export(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let wf = sqlx::query_as::<_, Workflow>(
        "SELECT * FROM flow.workflows WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| FlowError::NotFound("Workflow introuvable".into()))?;

    let definition = match wf.file_id {
        Some(fid) => cf::read_definition(&state, user.id, fid).await?,
        None => cf::empty_definition(),
    };

    Ok(Json(json!({
        "name":       wf.name,
        "kubuno":     true,
        "definition": definition,
        "tags":       wf.tags,
    })))
}

/// POST /import — importe un workflow (format Kubuno ou n8n).
pub async fn import(
    State(state): State<AppState>,
    user: FlowUserExt,
    Json(body): Json<Value>,
) -> Result<Json<Workflow>> {
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("Workflow importé").to_string();

    // Format Kubuno : { definition: { nodes, edges } }
    let definition: WorkflowDefinition = if let Some(def) = body.get("definition") {
        WorkflowDefinition::from_value(def)
    } else if body.get("connections").is_some() {
        // Format n8n : { nodes: [...], connections: {...} }
        convert_n8n(&body)
    } else {
        WorkflowDefinition::default()
    };

    let def_value = serde_json::to_value(&definition).unwrap_or_else(|_| json!({"nodes":[],"edges":[]}));

    // Définition → fichier .kbflw (dossier protégé Flow/).
    let file_id = cf::create_workflow_file(&state, user.id, &name, def_value.clone()).await?;

    let mut wf = sqlx::query_as::<_, Workflow>(
        r#"INSERT INTO flow.workflows (owner_id, name, description, file_id)
           VALUES ($1, $2, $3, $4) RETURNING *"#,
    )
    .bind(user.id)
    .bind(&name)
    .bind(Option::<&str>::None)
    .bind(file_id)
    .fetch_one(&state.db)
    .await?;
    wf.definition = def_value;

    Ok(Json(wf))
}

/// Conversion best-effort d'un workflow n8n vers le format Kubuno.
fn convert_n8n(body: &Value) -> WorkflowDefinition {
    use crate::models::workflow::{NodePosition, WorkflowEdge, WorkflowNode};

    let mut nodes = Vec::new();
    if let Some(arr) = body.get("nodes").and_then(|v| v.as_array()) {
        for n in arr {
            let name = n.get("name").and_then(|v| v.as_str()).unwrap_or("node").to_string();
            let pos = n.get("position").and_then(|v| v.as_array());
            let (x, y) = match pos {
                Some(p) if p.len() >= 2 => (p[0].as_f64().unwrap_or(0.0), p[1].as_f64().unwrap_or(0.0)),
                _ => (0.0, 0.0),
            };
            nodes.push(WorkflowNode {
                id: name.clone(),
                node_type: "external.http_request".to_string(), // type n8n non mappé → générique
                name: Some(name),
                position: NodePosition { x, y },
                config: n.get("parameters").cloned().unwrap_or(json!({})),
                settings: Default::default(),
            });
        }
    }

    let mut edges = Vec::new();
    if let Some(conns) = body.get("connections").and_then(|v| v.as_object()) {
        for (source, outs) in conns {
            if let Some(main) = outs.get("main").and_then(|v| v.as_array()) {
                for (port_idx, port) in main.iter().enumerate() {
                    if let Some(targets) = port.as_array() {
                        for t in targets {
                            if let Some(target) = t.get("node").and_then(|v| v.as_str()) {
                                edges.push(WorkflowEdge {
                                    id: format!("{source}-{target}-{port_idx}"),
                                    source: source.clone(),
                                    target: target.to_string(),
                                    source_port: None,
                                    target_port: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    WorkflowDefinition { nodes, edges }
}
