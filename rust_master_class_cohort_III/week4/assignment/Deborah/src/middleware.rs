use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use axum::{
    extract::Request,
    http::HeaderMap,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::error::ApiError;

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

pub async fn log_requests(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    let start = Instant::now();
    let response = next.run(req).await;
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

    let n = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    println!(
        "[req {n:4}] {:<6} {:<24} -> {} ({elapsed_ms:.2}ms)",
        method.as_str(),
        path,
        response.status().as_u16(),
    );

    response
}

// Byte-by-byte, non-short-circuiting comparison so a wrong API key takes the
// same time to reject regardless of how many leading bytes happen to match.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub async fn require_api_key(headers: HeaderMap, req: Request, next: Next) -> Response {
    let expected = std::env::var("API_KEY").unwrap_or_else(|_| "dev-secret-key".to_string());
    let provided = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
        next.run(req).await
    } else {
        ApiError::Unauthorized.into_response()
    }
}
