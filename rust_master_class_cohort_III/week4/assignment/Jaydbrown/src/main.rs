use axum::{
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Book {
    id: u64,
    title: String,
    author: String,
    genre: String,
    available: bool,
    added_at: String,
}

#[derive(Debug, Deserialize)]
struct NewBook {
    title: String,
    author: String,
    genre: String,
}

#[derive(Debug, Deserialize)]
struct PutBook {
    title: String,
    author: String,
    genre: String,
    available: bool,
}

#[derive(Debug, Deserialize)]
struct PatchBook {
    title: Option<String>,
    author: Option<String>,
    genre: Option<String>,
    available: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct FilterParams {
    genre: Option<String>,
    available: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    q: Option<String>,
    limit: Option<usize>,
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
}
