//! Week 4 · Day 5 — Blog API, middleware.
//!
//! Both layers are plain `async fn(Request, Next) -> Response`, lifted into a
//! `tower::Layer` by `middleware::from_fn_with_state`.
//!
//! `Next` is the rest of the stack. Calling `next.run(req).await` continues;
//! returning without calling it short-circuits, which is the entire mechanism
//! behind auth, rate limiting, and caching.
//!
//! Week 6 Day 4 replaces `from_fn` with a hand-written `Layer` + `Service` impl
//! for the rate limiter. Same idea, one level lower.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::{Request, State},
    http::HeaderMap,
    middleware::Next,
    response::Response,
};

use crate::errors::ApiError;
use crate::state::AppState;

/// Injected by [`log_requests`], read by handlers via `Extension<RequestId>`.
#[derive(Debug, Clone, Copy)]
pub struct RequestId(pub u64);

/// Logs `method path -> status (duration)` and attaches a `RequestId`.
///
/// Everything before `next.run()` runs on the way in; everything after runs on
/// the way out with the response in hand. Every middleware has that shape.
pub async fn log_requests(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Response {
    let id = RequestId(state.request_counter.fetch_add(1, Ordering::Relaxed));
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    // Extensions are a typed map on the request: the channel from middleware to
    // handler. Real auth middleware uses exactly this to pass the authenticated
    // user downstream.
    //
    // Caveat worth demonstrating: it resolves at runtime. Remove this layer and
    // any handler taking `Extension<RequestId>` fails with a 500, not a compile
    // error.
    req.extensions_mut().insert(id);

    let started = Instant::now();
    let response = next.run(req).await;

    println!(
        "[req {:>4}] {:<6} {:<26} -> {} ({:.2?})",
        id.0,
        method.as_str(),
        path,
        response.status().as_u16(),
        started.elapsed(),
    );

    response
}

/// Rejects requests without a valid `X-API-KEY`.
///
/// Returns `Result<Response, ApiError>`; on the error path `next` is never called,
/// so the handler never runs.
pub async fn require_api_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let provided = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .ok_or(ApiError::Unauthorized)?;

    if !constant_time_eq(provided.as_bytes(), state.api_key.as_bytes()) {
        return Err(ApiError::Unauthorized);
    }

    Ok(next.run(req).await)
}

/// Compares two byte strings without short-circuiting on the first mismatch.
///
/// `==` returns as soon as it finds a differing byte, so response timing leaks how
/// many leading bytes an attacker guessed correctly — turning an infeasible brute
/// force into a byte-at-a-time walk. Production code should use the `subtle` crate;
/// this is here so the habit registers.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;

    #[test]
    fn matches_identical_keys() {
        assert!(constant_time_eq(b"dev-secret-key", b"dev-secret-key"));
    }

    #[test]
    fn rejects_different_keys_of_equal_length() {
        assert!(!constant_time_eq(b"dev-secret-key", b"dev-secret-keZ"));
    }

    #[test]
    fn rejects_length_mismatch() {
        assert!(!constant_time_eq(b"short", b"much-longer-key"));
    }

    #[test]
    fn rejects_prefix() {
        assert!(!constant_time_eq(b"dev", b"dev-secret-key"));
    }

    #[test]
    fn rejects_empty_against_real_key() {
        assert!(!constant_time_eq(b"", b"dev-secret-key"));
    }
}
