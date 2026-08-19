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
///
/// This is the module's only unauthenticated entry point, so both instance
/// guards are applied BEFORE the token is even looked up: an instance that
/// forbids public webhooks must not become a token oracle, and an oversized
/// body must not be parsed.
pub async fn receive(
    State(state): State<AppState>,
    Path(token): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>> {
    let cfg = state.instance();
    if !cfg.allow_public_webhooks {
        return Err(FlowError::Forbidden);
    }
    let max_bytes = (cfg.webhook_max_body_kb.max(1) as usize).saturating_mul(1024);
    if body.len() > max_bytes {
        return Err(FlowError::Validation(format!(
            "Corps du webhook trop volumineux (limite : {} Kio)",
            cfg.webhook_max_body_kb
        )));
    }

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

    // A non-JSON (or empty) body keeps the previous meaning: `null`.
    let parsed: Value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap_or(Value::Null)
    };
    let trigger_data = json!({ "body": parsed });
    let job_id = queue::enqueue(
        &state.db,
        workflow_id,
        owner_id,
        "webhook",
        trigger_data,
        cfg.max_retries,
    )
    .await?;

    Ok(Json(json!({ "queued": true, "job_id": job_id })))
}
