//! Credentials CRUD. Secret values are AES-GCM encrypted at rest and NEVER
//! returned to the client — list/get expose metadata only.

use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{FlowError, Result},
    middleware::FlowUserExt,
    models::credential::{
        credential_catalog, Credential, CreateCredentialDto, CredentialMeta, UpdateCredentialDto,
    },
    services::crypto,
    state::AppState,
};

fn key(state: &AppState) -> [u8; 32] {
    crypto::derive_key(&state.settings.core.internal_secret)
}

/// GET /credential-types — catalogue of credential types (for the frontend).
pub async fn types() -> Json<Value> {
    Json(json!(credential_catalog()))
}

#[derive(serde::Deserialize)]
pub struct TestCredentialDto {
    #[serde(rename = "type")]
    pub type_id: String,
    #[serde(default)]
    pub data: Value,
}

/// POST /credentials/test — best-effort live test of a candidate credential
/// (connection / authenticated call). Does not require saving first.
pub async fn test(
    State(state): State<AppState>,
    _user: FlowUserExt,
    Json(dto): Json<TestCredentialDto>,
) -> Result<Json<Value>> {
    let r = crate::services::credentials::test(&state.proxy, &dto.type_id, &dto.data).await;
    Ok(Json(json!({ "ok": r.ok, "message": r.message })))
}

/// GET /credentials — list the user's credentials (metadata only).
pub async fn list(State(state): State<AppState>, user: FlowUserExt) -> Result<Json<Vec<CredentialMeta>>> {
    let rows = sqlx::query_as::<_, Credential>(
        "SELECT * FROM flow.credentials WHERE owner_id = $1 ORDER BY updated_at DESC",
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| { tracing::error!(error = %e, "list credentials"); e })?;
    Ok(Json(rows.iter().map(CredentialMeta::from).collect()))
}

/// POST /credentials — create (encrypts the payload).
pub async fn create(
    State(state): State<AppState>,
    user: FlowUserExt,
    Json(dto): Json<CreateCredentialDto>,
) -> Result<Json<CredentialMeta>> {
    if dto.name.trim().is_empty() {
        return Err(FlowError::Validation("Nom requis".into()));
    }
    if !credential_catalog().iter().any(|c| c.type_id == dto.type_id) {
        return Err(FlowError::Validation(format!("Type de credential inconnu : {}", dto.type_id)));
    }
    let plaintext = serde_json::to_vec(&dto.data).map_err(|e| FlowError::Validation(e.to_string()))?;
    let (ct, nonce) = crypto::encrypt(&key(&state), &plaintext).map_err(FlowError::Validation)?;

    let row = sqlx::query_as::<_, Credential>(
        r#"INSERT INTO flow.credentials (owner_id, name, type, data, nonce)
           VALUES ($1,$2,$3,$4,$5) RETURNING *"#,
    )
    .bind(user.id)
    .bind(dto.name.trim())
    .bind(&dto.type_id)
    .bind(&ct)
    .bind(&nonce)
    .fetch_one(&state.db)
    .await
    .map_err(|e| { tracing::error!(error = %e, "create credential"); e })?;
    Ok(Json((&row).into()))
}

/// PUT /credentials/:id — rename and/or replace the secret payload.
pub async fn update(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateCredentialDto>,
) -> Result<Json<CredentialMeta>> {
    let existing = sqlx::query_as::<_, Credential>(
        "SELECT * FROM flow.credentials WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| FlowError::NotFound("Credential introuvable".into()))?;

    let name = dto.name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or(existing.name);
    let (data, nonce) = match dto.data {
        Some(d) => {
            let pt = serde_json::to_vec(&d).map_err(|e| FlowError::Validation(e.to_string()))?;
            crypto::encrypt(&key(&state), &pt).map_err(FlowError::Validation)?
        }
        None => (existing.data, existing.nonce),
    };

    let row = sqlx::query_as::<_, Credential>(
        r#"UPDATE flow.credentials SET name = $2, data = $3, nonce = $4, updated_at = NOW()
           WHERE id = $1 RETURNING *"#,
    )
    .bind(id)
    .bind(&name)
    .bind(&data)
    .bind(&nonce)
    .fetch_one(&state.db)
    .await?;
    Ok(Json((&row).into()))
}

/// DELETE /credentials/:id
pub async fn delete(State(state): State<AppState>, user: FlowUserExt, Path(id): Path<Uuid>) -> Result<Json<Value>> {
    let res = sqlx::query("DELETE FROM flow.credentials WHERE id = $1 AND owner_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(FlowError::NotFound("Credential introuvable".into()));
    }
    Ok(Json(json!({ "deleted": true })))
}
