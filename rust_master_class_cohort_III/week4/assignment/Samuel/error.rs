use axum::{
    extract::Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("validation failed: {0}")]
    ValidationFailed(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("internal error")]
    #[allow(dead_code)]
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, kind, message) = match &self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg.clone()),
            ApiError::ValidationFailed(msg) => (StatusCode::BAD_REQUEST, "validation_failed", msg.clone()),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", "missing or invalid API key".into()),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg.clone()),
            ApiError::Internal(msg) => {
                eprintln!("[ERROR] {msg}");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "an internal error occurred".into())
            }
        };

        let body = serde_json::json!({
            "error": { "kind": kind, "message": message }
        });

        (status, Json(body)).into_response()
    }
}
