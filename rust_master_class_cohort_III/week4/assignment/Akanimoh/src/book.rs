use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::ApiError;

pub const MAX_TITLE_LEN: usize = 150;

#[derive(Debug, Clone, Serialize)]
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
pub struct NewBook {
    pub title: String,
    pub author: String,
    pub genre: String,
    pub available: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ReplaceBook {
    pub title: String,
    pub author: String,
    pub genre: String,
    pub available: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBook {
    pub title: Option<String>,
    pub author: Option<String>,
    pub genre: Option<String>,
    pub available: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct FilterParams {
    pub genre: Option<String>,
    pub available: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub q: Option<String>,
    pub limit: Option<usize>,
}

pub fn validate_title(title: &str) -> Result<(), ApiError> {
    if title.trim().is_empty() {
        return Err(ApiError::Validation("title must not be empty".into()));
    }
    if title.chars().count() > MAX_TITLE_LEN {
        return Err(ApiError::Validation(format!(
            "title must be {} characters or fewer",
            MAX_TITLE_LEN
        )));
    }
    Ok(())
}

pub fn validate_author(author: &str) -> Result<(), ApiError> {
    if author.trim().is_empty() {
        return Err(ApiError::Validation("author must not be empty".into()));
    }
    Ok(())
}

pub fn validate_genre(genre: &str) -> Result<(), ApiError> {
    if genre.trim().is_empty() {
        return Err(ApiError::Validation("genre must not be empty".into()));
    }
    Ok(())
}

pub fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    let (hour, minute, second) = (rest / 3600, (rest % 3600) / 60, rest % 60);
    let (year, month, day) = civil_from_days(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, minute, second
    )
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;

    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };

    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(civil_from_days(16_801), (2016, 1, 1));
    }

    #[test]
    fn timestamp_looks_like_rfc3339() {
        let stamp = now_rfc3339();

        assert_eq!(stamp.len(), 20);
        assert!(stamp.ends_with('Z'));
        assert!(stamp.contains('T'));
    }

    #[test]
    fn rejects_bad_titles() {
        assert!(validate_title("Clean Code").is_ok());
        assert!(validate_title("").is_err());
        assert!(validate_title("   ").is_err());
        assert!(validate_title(&"x".repeat(151)).is_err());
        assert!(validate_title(&"x".repeat(150)).is_ok());
    }
}
