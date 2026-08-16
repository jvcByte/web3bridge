use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::ApiError;

pub const MAX_TITLE_LENGTH: usize = 150;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Book {
    pub id: u64,
    pub title: String,
    pub author: String,
    pub genre: String,
    pub available: bool,
    pub added_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewBook {
    pub title: String,
    pub author: String,
    pub genre: String,
    pub available: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceBook {
    pub title: String,
    pub author: String,
    pub genre: String,
    pub available: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateBook {
    pub title: Option<String>,
    pub author: Option<String>,
    pub genre: Option<String>,
    pub available: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct FilterParams {
    pub genre: Option<String>,
    pub available: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SearchParams {
    pub q: Option<String>,
    pub limit: Option<usize>,
}

pub fn validate_title(title: &str) -> Result<(), ApiError> {
    if title.trim().is_empty() {
        return Err(ApiError::Validation(
            "title must contain at least one non-whitespace character".to_string(),
        ));
    }

    if title.chars().count() > MAX_TITLE_LENGTH {
        return Err(ApiError::Validation(format!(
            "title must be {MAX_TITLE_LENGTH} characters or fewer"
        )));
    }

    Ok(())
}

pub fn validate_author(author: &str) -> Result<(), ApiError> {
    if author.trim().is_empty() {
        return Err(ApiError::Validation(
            "author must contain at least one non-whitespace character".to_string(),
        ));
    }

    Ok(())
}

pub fn validate_genre(genre: &str) -> Result<(), ApiError> {
    if genre.trim().is_empty() {
        return Err(ApiError::Validation(
            "genre must contain at least one non-whitespace character".to_string(),
        ));
    }

    Ok(())
}

pub fn now_rfc3339() -> String {
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

    #[test]
    fn validates_all_required_text_fields() {
        assert!(validate_title("A valid title").is_ok());
        assert!(validate_title(" ").is_err());
        assert!(validate_title(&"x".repeat(150)).is_ok());
        assert!(validate_title(&"x".repeat(151)).is_err());
        assert!(validate_author("").is_err());
        assert!(validate_genre("\t").is_err());
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
