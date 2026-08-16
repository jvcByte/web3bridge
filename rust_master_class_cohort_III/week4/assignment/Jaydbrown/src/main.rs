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

struct Store {
    books: HashMap<u64, Book>,
    next_id: u64,
}

type SharedStore = Arc<Mutex<Store>>;

fn seed_store() -> Store {
    let mut books = HashMap::new();
    books.insert(
        1,
        Book {
            id: 1,
            title: "The Rust Programming Language".to_string(),
            author: "Steve Klabnik".to_string(),
            genre: "Technical".to_string(),
            available: true,
            added_at: now_rfc3339(),
        },
    );
    books.insert(
        2,
        Book {
            id: 2,
            title: "Programming Rust".to_string(),
            author: "Jim Blandy".to_string(),
            genre: "Technical".to_string(),
            available: false,
            added_at: now_rfc3339(),
        },
    );
    Store { books, next_id: 3 }
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
}
