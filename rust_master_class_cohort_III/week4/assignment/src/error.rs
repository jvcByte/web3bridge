use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::sync::PoisonError;

#[derive(Serialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    kind: &'static str,
    message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    ValidationFailed(String),

    #[error("{0}")]
    Unauthorized(String),

    #[error("{0}")]
    Conflict(String),

    #[error("{0}")]
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, kind, message) = match self {
            ApiError::NotFound(message) => (StatusCode::NOT_FOUND, "not_found", message),

            ApiError::ValidationFailed(message) => {
                (StatusCode::BAD_REQUEST, "validation_failed", message)
            }

            ApiError::Unauthorized(message) => (StatusCode::UNAUTHORIZED, "unauthorized", message),

            ApiError::Conflict(message) => (StatusCode::CONFLICT, "conflict", message),

            ApiError::Internal(details) => {
                eprintln!("internal error: {details}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "internal server error".to_string(),
                )
            }
        };

        let body = ErrorResponse {
            error: ErrorBody { kind, message },
        };

        (status, Json(body)).into_response()
    }
}

// Convert poisoned store locks into safe internal errors.
impl<T> From<PoisonError<T>> for ApiError {
    fn from(error: PoisonError<T>) -> Self {
        ApiError::Internal(error.to_string())
    }
}
