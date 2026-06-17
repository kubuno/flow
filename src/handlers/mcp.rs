//! MCP entrypoint. The core's MCP server exposes a `flow_run_workflow` tool and
//! proxies its calls here (with `X-Internal-Secret` + `X-Kubuno-User-Id`). We run
//! the named workflow synchronously and return its output. Only workflows that
//! contain a `trigger.mcp` node are runnable this way.

use axum::{extract::State, http::HeaderMap, Json};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{FlowError, Result},
    models::workflow::WorkflowDefinition,
    nodes::trait_::NodeContext,
    runtime::executor::run_workflow_inline,
    state::AppState,
};

fn header<'a>(h: &'a HeaderMap, k: &str) -> Option<&'a str> {
    h.get(k).and_then(|v| v.to_str().ok())
}

/// POST /mcp/run — { workflow_id, input } → workflow output. Internal-only.
pub async fn run(State(state): State<AppState>, headers: HeaderMap, Json(args): Json<Value>) -> Result<Json<Value>> {
    // Authentification interne (appel proxifié par le core, pas un JWT utilisateur).
    if header(&headers, "x-internal-secret") != Some(state.settings.core.internal_secret.as_str()) {
        return Err(FlowError::Forbidden);
    }
    let user_id = header(&headers, "x-kubuno-user-id")
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or(FlowError::Forbidden)?;

    let wf_id = args.get("workflow_id").and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s.trim()).ok())
        .ok_or_else(|| FlowError::Validation("workflow_id requis".into()))?;
    let input = args.get("input").cloned().unwrap_or_else(|| json!({}));

    let row = sqlx::query_as::<_, (Option<Uuid>,)>(
        "SELECT file_id FROM flow.workflows WHERE id = $1 AND owner_id = $2 AND is_trashed = FALSE",
    )
    .bind(wf_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| FlowError::NotFound("Workflow introuvable".into()))?;

    let def_val = match row.0 {
        Some(fid) => crate::services::content_files::read_definition(&state, user_id, fid).await?,
        None => crate::services::content_files::empty_definition(),
    };
    let definition = WorkflowDefinition::from_value(&def_val);

    // Garde-fou : seuls les workflows EXPLICITEMENT exposés (nœud trigger.mcp) sont appelables.
    if !definition.nodes.iter().any(|n| n.node_type == "trigger.mcp") {
        return Err(FlowError::Forbidden);
    }

    let node_ctx = NodeContext {
        proxy: &state.proxy, user_id, db: &state.db, settings: &state.settings,
        registry: &state.registry, files_client: &state.files_client, depth: 0,
    };
    let output = run_workflow_inline(&node_ctx, user_id, wf_id, &definition, input)
        .await
        .map_err(FlowError::Validation)?;
    Ok(Json(json!({ "ok": true, "output": output })))
}
