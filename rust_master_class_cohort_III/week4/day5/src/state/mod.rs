//! Week 4 · Day 5 — Blog API, shared application state.
//!
//! One `AppState`, wrapped in an `Arc`, handed to every handler by axum's `State`
//! extractor.
//!
//! The important design decision here: all mutable data lives behind **one**
//! mutex, and the id counter lives *inside* it. Two separate locks would let two
//! concurrent creates read the same id before either wrote, silently losing a
//! post. One lock makes "read the counter, then insert" a single atomic step.

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::errors::{ApiError, ApiResult};
use crate::models::Post;

/// The in-memory database.
///
/// A `HashMap` rather than a `Vec` because ids must be stable: with a vector
/// indexed by position, deleting post 2 renumbers post 3, and any client holding
/// the old id now silently reads the wrong row.
#[derive(Debug, Default)]
pub struct Store {
    pub posts: HashMap<u64, Post>,
    pub next_id: u64,
}

impl Store {
    pub fn seeded() -> Self {
        let mut store = Store {
            posts: HashMap::new(),
            next_id: 1,
        };
        store.insert_seed("Futures are lazy", "Calling an async fn runs none of its body.", "Ada");
        store.insert_seed(
            "join! vs spawn",
            "One task interleaved, versus N tasks needing Send + 'static.",
            "Grace",
        );
        store
    }

    fn insert_seed(&mut self, title: &str, body: &str, author: &str) {
        let id = self.next_id;
        self.next_id += 1;
        self.posts.insert(
            id,
            Post {
                id,
                title: title.into(),
                body: body.into(),
                author: author.into(),
                created_at: now_epoch_secs(),
            },
        );
    }

    /// Reserves the next id. Callable only with the lock held, since `&mut self`
    /// implies the guard.
    pub fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn title_exists(&self, title: &str, excluding: Option<u64>) -> bool {
        self.posts.values().any(|p| {
            p.title.eq_ignore_ascii_case(title.trim()) && Some(p.id) != excluding
        })
    }
}

/// Everything shared across requests.
pub struct AppState {
    store: Mutex<Store>,
    pub api_key: String,
    pub request_counter: AtomicU64,
}

impl AppState {
    pub fn new(api_key: impl Into<String>) -> Self {
        AppState {
            store: Mutex::new(Store::seeded()),
            api_key: api_key.into(),
            request_counter: AtomicU64::new(1),
        }
    }

    /// The only way to reach the store.
    ///
    /// Private field plus this accessor means every call site goes through the
    /// same poison-to-`ApiError` conversion, instead of scattering
    /// `.lock().unwrap()` across a dozen handlers where a panic in one poisons
    /// the rest.
    ///
    /// `std::sync::Mutex`, not `tokio::sync::Mutex`: nothing here is held across
    /// an `.await`. If someone tries, the guard's `!Send` bound turns it into a
    /// compile error rather than a production deadlock.
    pub fn store(&self) -> ApiResult<MutexGuard<'_, Store>> {
        self.store.lock().map_err(ApiError::lock_poisoned)
    }
}

/// Seconds since the Unix epoch.
///
/// The `expect` is unreachable short of a system clock set before 1970.
pub fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the Unix epoch")
        .as_secs()
}
