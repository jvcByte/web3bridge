//! Week 4 · Day 5 — Blog API, error types.
//!
//! One error enum for the whole application, plus the single `IntoResponse` impl
//! that decides how each variant is spoken over HTTP.
//!
//! Keeping those two concerns in separate places is deliberate: the enum says
//! *what went wrong*, the impl says *how to report it*. Swap the transport and
//! only the impl changes.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde_json::json;
use thiserror::Error;

/// Every failure the API can produce.
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0} not found")]
    NotFound(String),

    #[error("{0}")]
    Validation(String),

    #[error("missing or invalid X-API-KEY")]
    Unauthorized,

    #[error("{0} already exists")]
    Conflict(String),

    /// Catch-all. Its message is logged server-side and never sent to a client.
    #[error("internal error: {0}")]
    Internal(String),
}

impl ApiError {
    pub fn status(&self) -> StatusCode {
        match self {
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::Validation(_) => StatusCode::BAD_REQUEST,
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Stable machine-readable slug. Clients branch on this; the human-readable
    /// message stays free to be reworded.
    pub fn kind(&self) -> &'static str {
        match self {
            ApiError::NotFound(_) => "not_found",
            ApiError::Validation(_) => "validation_failed",
            ApiError::Unauthorized => "unauthorized",
            ApiError::Conflict(_) => "conflict",
            ApiError::Internal(_) => "internal_error",
        }
    }

    /// Helper for the common "the mutex was poisoned" case, which happens when
    /// another thread panicked while holding the lock.
    pub fn lock_poisoned(detail: impl std::fmt::Display) -> Self {
        ApiError::Internal(format!("store lock poisoned: {detail}"))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // Log the true cause, return a generic message. Internal errors routinely
        // carry connection strings, file paths, and query text; none of that
        // belongs in a response body.
        let message = match &self {
            ApiError::Internal(detail) => {
                eprintln!("  !! internal error: {detail}");
                "internal server error".to_string()
            }
            other => other.to_string(),
        };

        let body = Json(json!({
            "error": {
                "kind": self.kind(),
                "message": message,
            }
        }));

        (self.status(), body).into_response()
    }
}

/// Every handler in this crate returns this.
pub type ApiResult<T> = Result<T, ApiError>;
