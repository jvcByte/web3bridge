use std::collections::HashMap;
use std::sync::{Arc, Mutex, atomic::AtomicU64};

use crate::models::Book;

pub struct AppState {
    pub store: Mutex<BookStore>,
    pub request_counter: AtomicU64,
}

pub struct BookStore {
    pub books: HashMap<u64, Book>,
    pub next_id: u64,
}

pub type SharedState = Arc<AppState>;

pub fn seed_books() -> BookStore {
    let mut books = HashMap::new();

    books.insert(1, Book {
        id: 1,
        title: "The Rust Programming Language".into(),
        author: "Steve Klabnik".into(),
        genre: "Technical".into(),
        available: true,
        added_at: "2026-01-01T00:00:00Z".into(),
    });

    books.insert(2, Book {
        id: 2,
        title: "Programming Rust".into(),
        author: "Jim Blandy".into(),
        genre: "Technical".into(),
        available: false,
        added_at: "2026-01-01T00:00:00Z".into(),
    });

    BookStore { books, next_id: 3 }
}

pub fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let days = secs / 86400;
    let time = secs % 86400;

    let (year, month, day) = days_to_date(days);

    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        (time % 3600) / 60,
        time % 60,
    )
}

fn days_to_date(mut remaining: u64) -> (u64, u64, u64) {
    let mut y = 1970;
    loop {
        let ydays = if is_leap(y) { 366 } else { 365 };
        if remaining < ydays { break; }
        remaining -= ydays;
        y += 1;
    }

    let months = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut m = 0;
    for d in months {
        if remaining < d { break; }
        remaining -= d;
        m += 1;
    }

    (y, m + 1, remaining + 1)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
