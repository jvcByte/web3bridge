use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::sync::PoisonError;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Validation(String),
    #[error("missing or invalid API key")]
    Unauthorized,
    #[error("{0}")]
    Conflict(String),
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    kind: &'static str,
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, kind) = match &self {
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::Validation(_) => (StatusCode::BAD_REQUEST, "validation_failed"),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };

        // The real detail stays on the server. Internal errors never reach the client.
        let message = match &self {
            ApiError::Internal(detail) => {
                eprintln!("internal error: {detail}");
                "an internal error occurred".to_string()
            }
            other => other.to_string(),
        };

        (status, Json(ErrorBody { error: ErrorDetail { kind, message } })).into_response()
    }
}

// Lets `state.lock()?` inside a handler convert a poisoned-mutex error straight
// into an ApiError via the `?` operator's built-in `From::from` call.
impl<T> From<PoisonError<T>> for ApiError {
    fn from(err: PoisonError<T>) -> Self {
        ApiError::Internal(format!("mutex poisoned: {err}"))
    }
}
