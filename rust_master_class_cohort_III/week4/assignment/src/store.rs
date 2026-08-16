use crate::models::Book;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct Store {
    pub books: HashMap<u64, Book>,
    pub next_id: u64,
}

pub type SharedState = Arc<Mutex<Store>>;

impl Store {
    pub fn seeded() -> Self {
        let book1 = Book {
            id: 1,
            title: "The Rust Programming Language".to_string(),
            author: "Steve Klabnik".to_string(),
            genre: "Technical".to_string(),
            available: true,
            added_at: "2026-01-01T00:00:00Z".to_string(),
        };

        let book2 = Book {
            id: 2,
            title: "Programming Rust".to_string(),
            author: "Jim Blandy".to_string(),
            genre: "Technical".to_string(),
            available: false,
            added_at: "2026-01-01T00:00:00Z".to_string(),
        };

        let mut mapp = HashMap::new();

        mapp.insert(1, book1);
        mapp.insert(2, book2);

        Self {
            books: mapp,
            next_id: 3,
        }
    }
}
