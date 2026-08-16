use std::collections::HashMap;

use super::Book;
use crate::error::ApiError;
use crate::requests::{
    CreateBookRequest, ListBooksQuery, ReplaceBookRequest, SearchBooksQuery, UpdateBookRequest,
};

const DEFAULT_SEARCH_LIMIT: usize = 10;

pub struct Store {
    books: HashMap<u64, Book>,
    next_id: u64,
}

impl Store {
    pub fn seeded() -> Self {
        let first = Book::seeded(
            1,
            "The Rust Programming Language",
            "Steve Klabnik",
            "Technical",
            true,
        );
        let second = Book::seeded(2, "Programming Rust", "Jim Blandy", "Technical", false);

        Self {
            books: HashMap::from([(first.id(), first), (second.id(), second)]),
            next_id: 3,
        }
    }

    pub fn list_books(&self, query: &ListBooksQuery) -> Vec<Book> {
        let mut books: Vec<Book> = self
            .books
            .values()
            .filter(|book| book.matches_filter(query))
            .cloned()
            .collect();
        books.sort_by_key(Book::id);
        books
    }

    pub fn get_book(&self, id: u64) -> Result<Book, ApiError> {
        self.books.get(&id).cloned().ok_or(ApiError::NotFound(id))
    }

    pub fn search_books(&self, query: &SearchBooksQuery) -> Vec<Book> {
        let normalized_query = query.q.as_deref().unwrap_or_default().to_lowercase();
        let limit = query.limit.unwrap_or(DEFAULT_SEARCH_LIMIT);
        let mut books: Vec<Book> = self
            .books
            .values()
            .filter(|book| book.matches_search(&normalized_query))
            .cloned()
            .collect();
        books.sort_by_key(Book::id);
        books.truncate(limit);
        books
    }

    pub fn create_book(&mut self, request: CreateBookRequest) -> Result<Book, ApiError> {
        let book = Book::new(self.next_id, request)?;
        self.ensure_unique_title(book.title(), None)?;

        self.next_id += 1;
        self.books.insert(book.id(), book.clone());
        Ok(book)
    }

    pub fn replace_book(&mut self, id: u64, request: ReplaceBookRequest) -> Result<Book, ApiError> {
        let replacement = self
            .books
            .get(&id)
            .ok_or(ApiError::NotFound(id))?
            .replace(request)?;
        self.ensure_unique_title(replacement.title(), Some(id))?;

        self.books.insert(id, replacement.clone());
        Ok(replacement)
    }

    pub fn update_book(&mut self, id: u64, request: UpdateBookRequest) -> Result<Book, ApiError> {
        let updated = self
            .books
            .get(&id)
            .ok_or(ApiError::NotFound(id))?
            .update(request)?;
        self.ensure_unique_title(updated.title(), Some(id))?;

        self.books.insert(id, updated.clone());
        Ok(updated)
    }

    pub fn delete_book(&mut self, id: u64) -> Result<(), ApiError> {
        self.books.remove(&id).ok_or(ApiError::NotFound(id))?;
        Ok(())
    }

    pub fn book_count(&self) -> usize {
        self.books.len()
    }

    fn ensure_unique_title(&self, title: &str, ignored_id: Option<u64>) -> Result<(), ApiError> {
        let duplicate = self
            .books
            .values()
            .any(|book| book.title() == title && Some(book.id()) != ignored_id);

        if duplicate {
            return Err(ApiError::Conflict(format!(
                "a book titled \"{title}\" already exists"
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_request(title: &str) -> CreateBookRequest {
        CreateBookRequest {
            title: title.to_string(),
            author: "Author".to_string(),
            genre: "Technical".to_string(),
            available: None,
        }
    }

    #[test]
    fn seed_data_and_next_id_match_the_assignment() {
        let mut store = Store::seeded();

        assert_eq!(store.book_count(), 2);
        assert_eq!(
            store.get_book(1).unwrap().title(),
            "The Rust Programming Language"
        );
        assert!(!store.get_book(2).unwrap().is_available());
        assert_eq!(
            store
                .create_book(create_request("Clean Code"))
                .unwrap()
                .id(),
            3
        );
    }

    #[test]
    fn duplicate_titles_are_rejected() {
        let mut store = Store::seeded();

        assert!(store
            .create_book(create_request("The Rust Programming Language"))
            .is_err());
    }

    #[test]
    fn update_preserves_unsupplied_fields() {
        let mut store = Store::seeded();
        let original = store.get_book(1).unwrap();
        let updated = store
            .update_book(
                1,
                UpdateBookRequest {
                    title: None,
                    author: None,
                    genre: None,
                    available: Some(false),
                },
            )
            .unwrap();

        assert_eq!(updated.title(), original.title());
        assert_eq!(updated.author(), original.author());
        assert!(!updated.is_available());
    }

    #[test]
    fn list_and_search_results_are_sorted_and_filtered() {
        let store = Store::seeded();
        let unavailable = store.list_books(&ListBooksQuery {
            genre: Some("technical".to_string()),
            available: Some(false),
        });
        assert_eq!(unavailable.len(), 1);
        assert_eq!(unavailable[0].id(), 2);

        let results = store.search_books(&SearchBooksQuery {
            q: Some("rust".to_string()),
            limit: Some(1),
        });
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id(), 1);
    }
}
