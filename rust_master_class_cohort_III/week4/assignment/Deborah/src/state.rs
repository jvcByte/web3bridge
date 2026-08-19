use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::models::Book;

pub struct Store {
    pub books: BTreeMap<u64, Book>,
    pub next_id: u64,
}

pub type AppState = Arc<Mutex<Store>>;

pub fn seeded_state() -> AppState {
    let mut books = BTreeMap::new();
    books.insert(
        1,
        Book {
            id: 1,
            title: "The Rust Programming Language".to_string(),
            author: "Steve Klabnik".to_string(),
            genre: "Technical".to_string(),
            available: true,
            added_at: "2024-01-01T00:00:00Z".to_string(),
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
            added_at: "2024-01-01T00:00:00Z".to_string(),
        },
    );

    Arc::new(Mutex::new(Store { books, next_id: 3 }))
}
