use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("book {0} not found")]
    NotFound(u64),

    #[error("no route for {0}")]
    RouteNotFound(String),

    #[error("{0}")]
    Validation(String),

    #[error("missing or invalid API key")]
    Unauthorized,

    #[error("{0}")]
    Conflict(String),

    #[error("{0}")]
    MethodNotAllowed(String),

    #[error("{0}")]
    Internal(String),
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            Self::NotFound(_) | Self::RouteNotFound(_) => StatusCode::NOT_FOUND,
            Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::MethodNotAllowed(_) => StatusCode::METHOD_NOT_ALLOWED,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::NotFound(_) | Self::RouteNotFound(_) => "not_found",
            Self::Validation(_) => "validation_failed",
            Self::Unauthorized => "unauthorized",
            Self::Conflict(_) => "conflict",
            Self::MethodNotAllowed(_) => "method_not_allowed",
            Self::Internal(_) => "internal_error",
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let kind = self.kind();
        let message = match &self {
            Self::Internal(cause) => {
                eprintln!("internal error: {cause}");
                "an internal server error occurred".to_string()
            }
            error => error.to_string(),
        };

        let body = json!({
            "error": {
                "kind": kind,
                "message": message,
            }
        });

        (status, Json(body)).into_response()
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        Self::Validation(format!("invalid JSON body: {}", rejection.body_text()))
    }
}

impl From<QueryRejection> for ApiError {
    fn from(rejection: QueryRejection) -> Self {
        Self::Validation(format!(
            "invalid query parameters: {}",
            rejection.body_text()
        ))
    }
}

impl From<PathRejection> for ApiError {
    fn from(rejection: PathRejection) -> Self {
        Self::Validation(format!("invalid path parameter: {}", rejection.body_text()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn response_body(error: ApiError) -> String {
        let response = error.into_response();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn not_found_uses_the_required_error_envelope() {
        let body = response_body(ApiError::NotFound(42)).await;

        assert_eq!(
            body,
            r#"{"error":{"kind":"not_found","message":"book 42 not found"}}"#
        );
    }

    #[tokio::test]
    async fn internal_error_hides_its_cause() {
        let body = response_body(ApiError::Internal(
            "/private/database password=secret".to_string(),
        ))
        .await;

        assert!(body.contains("internal_error"));
        assert!(!body.contains("/private/database"));
        assert!(!body.contains("secret"));
    }
}
