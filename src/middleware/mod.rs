use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

use crate::{errors::FlowError, state::AppState};

/// Utilisateur extrait des headers injectés par le core.
#[derive(Debug, Clone)]
pub struct FlowUser {
    pub id:    Uuid,
    pub role:  String,
    pub email: String,
}

pub type FlowUserExt = axum::Extension<FlowUser>;

/// Middleware : extrait X-Kubuno-User-Id / Role / Email injectés par le proxy core.
pub async fn require_auth(
    State(_state): State<AppState>,
    mut req: Request,
    next: Next,
) -> std::result::Result<Response, FlowError> {
    let user_id = req
        .headers()
        .get("x-kubuno-user-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or(FlowError::Unauthorized)?;

    let role = req
        .headers()
        .get("x-kubuno-user-role")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("user")
        .to_string();

    let email = req
        .headers()
        .get("x-kubuno-user-email")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    req.extensions_mut().insert(FlowUser { id: user_id, role, email });
    Ok(next.run(req).await)
}
