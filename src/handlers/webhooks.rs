use axum::{
    extract::{Path, State},
    Json,
};
use kubuno_db::params;
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
    // Vérifier l'appartenance du workflow (SELECT id plutôt qu'un EXISTS booléen,
    // dont le décodage diffère entre moteurs).
    let owns = state.db.fetch_optional_scalar::<Uuid>(
        "SELECT id FROM flow.workflows WHERE id = $1 AND owner_id = $2",
        params![id, user.id],
    )
    .await?
    .is_some();
    if !owns {
        return Err(FlowError::NotFound("Workflow introuvable".into()));
    }

    // Réutiliser un token existant pour ce nœud, sinon en créer un.
    let existing: Option<String> = state.db.fetch_optional_scalar::<String>(
        "SELECT token FROM flow.webhooks WHERE workflow_id = $1 AND node_id = $2",
        params![id, &body.node_id],
    )
    .await?;

    let token = match existing {
        Some(t) => t,
        None => {
            let t = gen_token();
            state.db.execute(
                "INSERT INTO flow.webhooks (token, workflow_id, node_id, owner_id) VALUES ($1,$2,$3,$4)",
                params![&t, id, &body.node_id, user.id],
            )
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

    let row = state.db.fetch_optional_as::<(Uuid, Uuid)>(
        "SELECT workflow_id, owner_id FROM flow.webhooks WHERE token = $1",
        params![&token],
    )
    .await?
    .ok_or_else(|| FlowError::NotFound("Webhook inconnu".into()))?;

    let (workflow_id, owner_id) = row;

    // Le workflow doit être actif. On lit le statut et on compare en Rust (une
    // comparaison booléenne en SQL se décode différemment selon le moteur).
    let active = state.db.fetch_optional_scalar::<String>(
        "SELECT status FROM flow.workflows WHERE id = $1 AND is_trashed = FALSE",
        params![workflow_id],
    )
    .await?
    .map(|s| s == "active")
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
