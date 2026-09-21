use axum::{
    extract::{Path, Query, State},
    Json,
};
use kubuno_db::params;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{FlowError, Result},
    middleware::FlowUserExt,
    models::execution::{Execution, NodeLog},
    state::AppState,
};

#[derive(Deserialize)]
pub struct ListParams {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}
fn default_limit() -> i64 { 50 }

/// GET /workflows/:id/executions
pub async fn list_for_workflow(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
    Query(p): Query<ListParams>,
) -> Result<Json<Vec<Execution>>> {
    let execs = state.db.fetch_all_as::<Execution>(
        "SELECT * FROM flow.executions \
           WHERE workflow_id = $1 AND owner_id = $2 \
           ORDER BY started_at DESC LIMIT $3 OFFSET $4",
        params![id, user.id, p.limit.clamp(1, 200), p.offset.max(0)],
    )
    .await?;
    Ok(Json(execs))
}

/// GET /executions/:id — détail + logs des nœuds.
pub async fn detail(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let exec = state.db.fetch_optional_as::<Execution>(
        "SELECT * FROM flow.executions WHERE id = $1 AND owner_id = $2",
        params![id, user.id],
    )
    .await?
    .ok_or_else(|| FlowError::NotFound("Exécution introuvable".into()))?;

    let logs = state.db.fetch_all_as::<NodeLog>(
        "SELECT * FROM flow.node_logs WHERE execution_id = $1 ORDER BY executed_at ASC",
        params![id],
    )
    .await?;

    Ok(Json(json!({ "execution": exec, "node_logs": logs })))
}

/// DELETE /executions/:id
pub async fn delete(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let affected = state.db.execute(
        "DELETE FROM flow.executions WHERE id = $1 AND owner_id = $2",
        params![id, user.id],
    )
    .await?;
    if affected == 0 {
        return Err(FlowError::NotFound("Exécution introuvable".into()));
    }
    Ok(Json(json!({ "deleted": true })))
}
