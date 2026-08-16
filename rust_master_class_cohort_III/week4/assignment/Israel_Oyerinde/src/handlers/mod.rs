mod books;
mod fallback;
mod health;

pub use books::{
    create_book, delete_book, get_book, list_books, replace_book, search_books, update_book,
};
pub use fallback::{method_not_allowed, not_found};
pub use health::health;

use std::sync::MutexGuard;

use crate::domain::Store;
use crate::error::ApiError;
use crate::AppState;

fn lock_store(state: &AppState) -> Result<MutexGuard<'_, Store>, ApiError> {
    state
        .store
        .lock()
        .map_err(|error| ApiError::Internal(format!("book store lock was poisoned: {error}")))
}
