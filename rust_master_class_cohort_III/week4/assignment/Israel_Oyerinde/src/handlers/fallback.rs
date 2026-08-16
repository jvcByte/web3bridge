use axum::http::{Method, Uri};

use crate::error::ApiError;

pub async fn not_found(uri: Uri) -> Result<(), ApiError> {
    Err(ApiError::RouteNotFound(uri.path().to_string()))
}

pub async fn method_not_allowed(method: Method, uri: Uri) -> Result<(), ApiError> {
    Err(ApiError::MethodNotAllowed(format!(
        "{method} is not allowed on {}",
        uri.path()
    )))
}
