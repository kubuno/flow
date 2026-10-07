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
/// This module's id, used as the token audience.
const MODULE_ID: &str = "flow";

/// Authenticate the caller from the signed `X-Kubuno-Auth` token the core
/// mints with this module's internal secret (see `kubuno-modauth`), instead of
/// trusting the plain `X-Kubuno-User-*` headers — which any process reaching this
/// module's loopback port could forge to impersonate any user.
pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> std::result::Result<Response, FlowError> {
    let token = req
        .headers()
        .get(kubuno_modauth::TOKEN_HEADER)
        .and_then(|v| v.to_str().ok())
        .ok_or(FlowError::Unauthorized)?;

    let user = kubuno_modauth::verify(
        state.settings.core.internal_secret.as_bytes(),
        token,
        MODULE_ID,
    )
    .map_err(|_| FlowError::Unauthorized)?;

    req.extensions_mut()
        .insert(FlowUser { id: user.id, role: user.role, email: user.email });
    Ok(next.run(req).await)
}
