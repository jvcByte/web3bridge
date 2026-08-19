use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::ApiError;
use crate::requests::{
    CreateBookRequest, ListBooksQuery, ReplaceBookRequest, UpdateBookRequest, Validate,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Book {
    id: u64,
    title: String,
    author: String,
    genre: String,
    available: bool,
    added_at: String,
}

impl Book {
    pub fn new(id: u64, request: CreateBookRequest) -> Result<Self, ApiError> {
        request.validate()?;

        Ok(Self {
            id,
            title: request.title,
            author: request.author,
            genre: request.genre,
            available: request.available.unwrap_or(true),
            added_at: now_rfc3339(),
        })
    }

    pub fn replace(&self, request: ReplaceBookRequest) -> Result<Self, ApiError> {
        request.validate()?;

        Ok(Self {
            id: self.id,
            title: request.title,
            author: request.author,
            genre: request.genre,
            available: request.available,
            added_at: self.added_at.clone(),
        })
    }

    pub fn update(&self, request: UpdateBookRequest) -> Result<Self, ApiError> {
        request.validate()?;

        let mut updated = self.clone();
        if let Some(title) = request.title {
            updated.title = title;
        }
        if let Some(author) = request.author {
            updated.author = author;
        }
        if let Some(genre) = request.genre {
            updated.genre = genre;
        }
        if let Some(available) = request.available {
            updated.available = available;
        }

        Ok(updated)
    }

    pub fn matches_filter(&self, query: &ListBooksQuery) -> bool {
        let genre_matches = query
            .genre
            .as_ref()
            .map(|genre| self.genre.eq_ignore_ascii_case(genre))
            .unwrap_or(true);
        let availability_matches = query
            .available
            .map(|available| self.available == available)
            .unwrap_or(true);

        genre_matches && availability_matches
    }

    pub fn matches_search(&self, normalized_query: &str) -> bool {
        self.title.to_lowercase().contains(normalized_query)
            || self.author.to_lowercase().contains(normalized_query)
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn author(&self) -> &str {
        &self.author
    }

    pub fn genre(&self) -> &str {
        &self.genre
    }

    pub fn is_available(&self) -> bool {
        self.available
    }

    pub fn added_at(&self) -> &str {
        &self.added_at
    }

    pub(crate) fn seeded(id: u64, title: &str, author: &str, genre: &str, available: bool) -> Self {
        Self {
            id,
            title: title.to_string(),
            author: author.to_string(),
            genre: genre.to_string(),
            available,
            added_at: now_rfc3339(),
        }
    }
}

fn now_rfc3339() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);

    unix_seconds_to_rfc3339(seconds)
}

fn unix_seconds_to_rfc3339(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let seconds_today = seconds % 86_400;
    let hour = seconds_today / 3_600;
    let minute = (seconds_today % 3_600) / 60;
    let second = seconds_today % 60;
    let (year, month, day) = civil_date_from_days(days);

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_date_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let shifted_days = days_since_epoch + 719_468;
    let era = shifted_days.div_euclid(146_097);
    let day_of_era = shifted_days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    year += i64::from(month <= 2);

    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_request() -> CreateBookRequest {
        CreateBookRequest {
            title: "Clean Code".to_string(),
            author: "Robert C. Martin".to_string(),
            genre: "Technical".to_string(),
            available: None,
        }
    }

    #[test]
    fn new_book_validates_input_and_defaults_to_available() {
        let book = Book::new(3, create_request()).unwrap();

        assert_eq!(book.id(), 3);
        assert_eq!(book.title(), "Clean Code");
        assert!(book.is_available());
        assert_eq!(book.added_at().len(), 20);
    }

    #[test]
    fn replacement_preserves_server_owned_fields() {
        let book = Book::new(3, create_request()).unwrap();
        let original_timestamp = book.added_at().to_string();
        let replacement = book
            .replace(ReplaceBookRequest {
                title: "Clean Code, Second Edition".to_string(),
                author: "Robert C. Martin".to_string(),
                genre: "Technical".to_string(),
                available: false,
            })
            .unwrap();

        assert_eq!(replacement.id(), 3);
        assert_eq!(replacement.added_at(), original_timestamp);
        assert!(!replacement.is_available());
    }

    #[test]
    fn partial_update_changes_only_supplied_fields() {
        let book = Book::new(3, create_request()).unwrap();
        let updated = book
            .update(UpdateBookRequest {
                title: None,
                author: None,
                genre: None,
                available: Some(false),
            })
            .unwrap();

        assert_eq!(updated.title(), book.title());
        assert_eq!(updated.author(), book.author());
        assert!(!updated.is_available());
    }

    #[test]
    fn formats_unix_time_as_rfc3339_utc() {
        assert_eq!(unix_seconds_to_rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(
            unix_seconds_to_rfc3339(1_709_164_800),
            "2024-02-29T00:00:00Z"
        );
    }
}
