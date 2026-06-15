use axum::{
    extract::{Path, Query, State},
    Json,
};
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
    let execs = sqlx::query_as::<_, Execution>(
        r#"SELECT * FROM flow.executions
           WHERE workflow_id = $1 AND owner_id = $2
           ORDER BY started_at DESC LIMIT $3 OFFSET $4"#,
    )
    .bind(id)
    .bind(user.id)
    .bind(p.limit.clamp(1, 200))
    .bind(p.offset.max(0))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(execs))
}

/// GET /executions/:id — détail + logs des nœuds.
pub async fn detail(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let exec = sqlx::query_as::<_, Execution>(
        "SELECT * FROM flow.executions WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| FlowError::NotFound("Exécution introuvable".into()))?;

    let logs = sqlx::query_as::<_, NodeLog>(
        "SELECT * FROM flow.node_logs WHERE execution_id = $1 ORDER BY executed_at ASC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "execution": exec, "node_logs": logs })))
}

/// DELETE /executions/:id
pub async fn delete(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let res = sqlx::query("DELETE FROM flow.executions WHERE id = $1 AND owner_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(FlowError::NotFound("Exécution introuvable".into()));
    }
    Ok(Json(json!({ "deleted": true })))
}
