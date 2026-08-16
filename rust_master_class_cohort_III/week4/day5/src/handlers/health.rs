//! Health and metadata endpoints. No auth, no state mutation.

use std::sync::Arc;

use axum::{extract::State, response::Json};
use serde::Serialize;
use serde_json::{json, Value};

use crate::errors::ApiResult;
use crate::state::AppState;

pub async fn root() -> &'static str {
    "Web3Bridge Blog API v1 — try /health, /about, /posts"
}

/// Reports whether the store lock is reachable, which is the only dependency this
/// service has. A health check that always returns `{"status":"ok"}` without
/// touching anything is decoration — it cannot fail, so it cannot inform.
pub async fn health(State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let store = state.store()?;

    Ok(Json(json!({
        "status": "ok",
        "posts": store.posts.len(),
    })))
}

#[derive(Debug, Serialize)]
pub struct About {
    name: &'static str,
    version: &'static str,
    cohort: &'static str,
    framework: &'static str,
}

pub async fn about() -> Json<About> {
    Json(About {
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        cohort: "rust_master_class_cohort_III",
        framework: "axum 0.8 on tokio",
    })
}
