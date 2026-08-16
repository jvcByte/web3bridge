//! Week 4 · Day 5 — Blog API, domain models.
//!
//! Three separate types for what a naive design would make one:
//!
//! - `Post`       — what we store and send back  (`Serialize`)
//! - `CreatePost` — what a POST/PUT accepts      (`Deserialize`, all fields required)
//! - `UpdatePost` — what a PATCH accepts         (`Deserialize`, all fields optional)
//!
//! The split is not ceremony. It is what makes it *structurally impossible* for a
//! client to set its own `id` or forge a `created_at` — those fields do not exist
//! on the input types, so no validation is needed to reject them.

use serde::{Deserialize, Serialize};

use crate::errors::{ApiError, ApiResult};

/// A blog post as stored and as returned.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Post {
    pub id: u64,
    pub title: String,
    pub body: String,
    pub author: String,
    /// Unix epoch seconds.
    ///
    /// Deliberately not an RFC-3339 string: that would need `chrono` or `time`,
    /// and this crate stays on `std` so students can read every dependency. An
    /// epoch integer is a legitimate wire format — plenty of production APIs use
    /// it. Swapping in `chrono::DateTime<Utc>` is a good homework exercise.
    pub created_at: u64,
}

/// Request body for `POST /posts` and `PUT /posts/{id}`.
#[derive(Debug, Deserialize)]
pub struct CreatePost {
    pub title: String,
    pub body: String,
    #[serde(default = "anonymous")]
    pub author: String,
}

/// Request body for `PATCH /posts/{id}`. Absent field means "leave it alone".
#[derive(Debug, Deserialize)]
pub struct UpdatePost {
    pub title: Option<String>,
    pub body: Option<String>,
    pub author: Option<String>,
}

fn anonymous() -> String {
    "anonymous".to_string()
}

pub const MAX_TITLE_LEN: usize = 120;
pub const MAX_BODY_LEN: usize = 10_000;

/// Shared field rules, so `CreatePost` and `UpdatePost` cannot drift apart.
fn check_title(title: &str) -> ApiResult<()> {
    if title.trim().is_empty() {
        return Err(ApiError::Validation("title must not be empty".into()));
    }
    if title.chars().count() > MAX_TITLE_LEN {
        return Err(ApiError::Validation(format!(
            "title must be {MAX_TITLE_LEN} characters or fewer"
        )));
    }
    Ok(())
}

fn check_body(body: &str) -> ApiResult<()> {
    if body.trim().is_empty() {
        return Err(ApiError::Validation("body must not be empty".into()));
    }
    if body.chars().count() > MAX_BODY_LEN {
        return Err(ApiError::Validation(format!(
            "body must be {MAX_BODY_LEN} characters or fewer"
        )));
    }
    Ok(())
}

impl CreatePost {
    pub fn validate(&self) -> ApiResult<()> {
        check_title(&self.title)?;
        check_body(&self.body)?;
        Ok(())
    }
}

impl UpdatePost {
    /// Validates only the fields actually supplied — that is the whole PATCH
    /// semantic, expressed as `if let Some`.
    pub fn validate(&self) -> ApiResult<()> {
        if let Some(title) = &self.title {
            check_title(title)?;
        }
        if let Some(body) = &self.body {
            check_body(body)?;
        }
        Ok(())
    }

    /// True when the client sent `{}` — nothing to do, which is worth a 400
    /// rather than a silent 200 that changed nothing.
    pub fn is_empty(&self) -> bool {
        self.title.is_none() && self.body.is_none() && self.author.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> CreatePost {
        CreatePost {
            title: "A title".into(),
            body: "A body".into(),
            author: "Ada".into(),
        }
    }

    #[test]
    fn accepts_a_reasonable_post() {
        assert!(valid().validate().is_ok());
    }

    #[test]
    fn rejects_empty_title() {
        let post = CreatePost {
            title: "   ".into(),
            ..valid()
        };
        assert!(matches!(
            post.validate(),
            Err(ApiError::Validation(_))
        ));
    }

    #[test]
    fn rejects_overlong_title() {
        let post = CreatePost {
            title: "x".repeat(MAX_TITLE_LEN + 1),
            ..valid()
        };
        assert!(post.validate().is_err());
    }

    #[test]
    fn counts_chars_not_bytes() {
        // A multi-byte char must count as one. `.len()` would count 3 bytes each
        // and reject this wrongly — the reason the checks use `.chars().count()`.
        let post = CreatePost {
            title: "é".repeat(MAX_TITLE_LEN),
            ..valid()
        };
        assert!(post.validate().is_ok());
    }

    #[test]
    fn patch_ignores_absent_fields() {
        let patch = UpdatePost {
            title: None,
            body: None,
            author: Some("Grace".into()),
        };
        assert!(patch.validate().is_ok());
        assert!(!patch.is_empty());
    }

    #[test]
    fn patch_rejects_supplied_empty_title() {
        let patch = UpdatePost {
            title: Some("".into()),
            body: None,
            author: None,
        };
        assert!(patch.validate().is_err());
    }
}
