use axum::{
    extract::{Path, State},
    Json,
};
use rand::RngCore;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{FlowError, Result},
    middleware::FlowUserExt,
    runtime::queue,
    state::AppState,
};

#[derive(Deserialize)]
pub struct RegisterWebhookBody {
    pub node_id: String,
}

fn gen_token() -> String {
    let mut buf = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

/// POST /workflows/:id/webhook — crée (ou retourne) un token de webhook pour un nœud.
pub async fn register(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
    Json(body): Json<RegisterWebhookBody>,
) -> Result<Json<Value>> {
    // Vérifier l'appartenance du workflow.
    let owns = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM flow.workflows WHERE id = $1 AND owner_id = $2)",
    )
    .bind(id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;
    if !owns {
        return Err(FlowError::NotFound("Workflow introuvable".into()));
    }

    // Réutiliser un token existant pour ce nœud, sinon en créer un.
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT token FROM flow.webhooks WHERE workflow_id = $1 AND node_id = $2",
    )
    .bind(id)
    .bind(&body.node_id)
    .fetch_optional(&state.db)
    .await?;

    let token = match existing {
        Some(t) => t,
        None => {
            let t = gen_token();
            sqlx::query(
                "INSERT INTO flow.webhooks (token, workflow_id, node_id, owner_id) VALUES ($1,$2,$3,$4)",
            )
            .bind(&t)
            .bind(id)
            .bind(&body.node_id)
            .bind(user.id)
            .execute(&state.db)
            .await?;
            t
        }
    };

    Ok(Json(json!({
        "token": token,
        "path":  format!("/api/v1/flow/webhook/{token}"),
    })))
}

/// POST|GET /webhook/:token — réception publique d'un webhook (sans auth).
pub async fn receive(
    State(state): State<AppState>,
    Path(token): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>> {
    let row = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT workflow_id, owner_id FROM flow.webhooks WHERE token = $1",
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| FlowError::NotFound("Webhook inconnu".into()))?;

    let (workflow_id, owner_id) = row;

    // Le workflow doit être actif.
    let active = sqlx::query_scalar::<_, bool>(
        "SELECT status = 'active' FROM flow.workflows WHERE id = $1 AND is_trashed = FALSE",
    )
    .bind(workflow_id)
    .fetch_optional(&state.db)
    .await?
    .unwrap_or(false);
    if !active {
        return Err(FlowError::Forbidden);
    }

    let trigger_data = json!({ "body": body.map(|b| b.0).unwrap_or(Value::Null) });
    let job_id = queue::enqueue(
        &state.db,
        workflow_id,
        owner_id,
        "webhook",
        trigger_data,
        state.settings.runtime.max_retries,
    )
    .await?;

    Ok(Json(json!({ "queued": true, "job_id": job_id })))
}
