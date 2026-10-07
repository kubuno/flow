use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum FlowError {
    #[error("Non authentifié")]
    Unauthorized,

    #[error("Accès refusé")]
    Forbidden,

    #[error("Ressource introuvable: {0}")]
    NotFound(String),

    #[error("Données invalides: {0}")]
    Validation(String),

    #[error("Conflit: {0}")]
    Conflict(String),

    #[error("Erreur base de données")]
    Database(#[from] sqlx::Error),

    #[error("Erreur interne")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for FlowError {
    fn into_response(self) -> Response {
        let (status, code, msg) = match &self {
            FlowError::Unauthorized  => (StatusCode::UNAUTHORIZED,         "UNAUTHORIZED", self.to_string()),
            FlowError::Forbidden     => (StatusCode::FORBIDDEN,            "FORBIDDEN",    self.to_string()),
            FlowError::NotFound(_)   => (StatusCode::NOT_FOUND,            "NOT_FOUND",    self.to_string()),
            FlowError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION",   self.to_string()),
            FlowError::Conflict(_)   => (StatusCode::CONFLICT,             "CONFLICT",     self.to_string()),
            FlowError::Database(e) => {
                tracing::error!(error = %e, "Database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR", "Erreur base de données".to_string())
            }
            FlowError::Internal(e) => {
                tracing::error!(error = %e, "Internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", "Erreur interne".to_string())
            }
        };
        (status, Json(json!({ "error": code, "message": msg }))).into_response()
    }
}

pub type Result<T> = std::result::Result<T, FlowError>;
