use axum::{
    extract::State,
    http::{HeaderMap, Method, Uri},
    middleware::Next,
    response::Response,
};
use std::sync::atomic::Ordering;
use std::time::Instant;

use crate::error::ApiError;
use crate::state::SharedState;

pub async fn log_requests(
    State(state): State<SharedState>,
    method: Method,
    uri: Uri,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let req_num = state.request_counter.fetch_add(1, Ordering::SeqCst) + 1;
    let start = Instant::now();
    let response = next.run(request).await;
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    println!(
        "[req {:>4}] {:<6} {:<25} -> {} ({:.2}ms)",
        req_num,
        method,
        uri.path(),
        response.status().as_u16(),
        elapsed,
    );

    response
}

pub async fn auth_middleware(
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Result<Response, ApiError> {
    let expected = std::env::var("API_KEY").unwrap_or_else(|_| "dev-secret-key".into());
    let provided = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
        return Err(ApiError::Unauthorized);
    }

    Ok(next.run(request).await)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}
