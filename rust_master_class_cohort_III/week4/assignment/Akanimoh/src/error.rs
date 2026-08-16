use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("book {0} not found")]
    NotFound(u64),

    #[error("{0}")]
    NoRoute(String),

    #[error("{0}")]
    MethodNotAllowed(String),

    #[error("{0}")]
    Validation(String),

    #[error("missing or invalid API key")]
    Unauthorized,

    #[error("{0}")]
    Conflict(String),

    #[error("{0}")]
    Internal(String),
}

impl ApiError {
    fn kind(&self) -> &'static str {
        match self {
            ApiError::NotFound(_) | ApiError::NoRoute(_) => "not_found",
            ApiError::MethodNotAllowed(_) => "method_not_allowed",
            ApiError::Validation(_) => "validation_failed",
            ApiError::Unauthorized => "unauthorized",
            ApiError::Conflict(_) => "conflict",
            ApiError::Internal(_) => "internal_error",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            ApiError::NotFound(_) | ApiError::NoRoute(_) => StatusCode::NOT_FOUND,
            ApiError::MethodNotAllowed(_) => StatusCode::METHOD_NOT_ALLOWED,
            ApiError::Validation(_) => StatusCode::BAD_REQUEST,
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let message = match &self {
            ApiError::Internal(cause) => {
                eprintln!("internal error: {}", cause);
                "something went wrong".to_string()
            }
            other => other.to_string(),
        };

        let body = json!({
            "error": {
                "kind": self.kind(),
                "message": message,
            }
        });

        (self.status(), Json(body)).into_response()
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        ApiError::Validation(rejection.body_text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn body_of(error: ApiError) -> String {
        let response = error.into_response();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn internal_errors_keep_the_cause_off_the_wire() {
        let secret = "/Users/admin/secret/db.sqlite password=hunter2";
        let body = body_of(ApiError::Internal(secret.to_string())).await;

        assert!(!body.contains("hunter2"));
        assert!(!body.contains("/Users/admin"));
        assert!(body.contains("internal_error"));
        assert!(body.contains("something went wrong"));
    }

    #[tokio::test]
    async fn each_variant_maps_to_its_kind_and_status() {
        assert_eq!(ApiError::NotFound(42).status(), StatusCode::NOT_FOUND);
        assert_eq!(ApiError::Unauthorized.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            ApiError::Validation("bad".into()).status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ApiError::Conflict("dup".into()).status(),
            StatusCode::CONFLICT
        );

        let body = body_of(ApiError::NotFound(42)).await;
        assert_eq!(
            body,
            r#"{"error":{"kind":"not_found","message":"book 42 not found"}}"#
        );
    }
}
