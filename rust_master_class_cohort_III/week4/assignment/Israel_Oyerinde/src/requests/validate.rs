use crate::error::ApiError;

pub const MAX_TITLE_LENGTH: usize = 150;

pub trait Validate {
    fn validate(&self) -> Result<(), ApiError>;
}

pub(crate) fn validate_title(title: &str) -> Result<(), ApiError> {
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

pub(crate) fn validate_author(author: &str) -> Result<(), ApiError> {
    if author.trim().is_empty() {
        return Err(ApiError::Validation(
            "author must contain at least one non-whitespace character".to_string(),
        ));
    }

    Ok(())
}

pub(crate) fn validate_genre(genre: &str) -> Result<(), ApiError> {
    if genre.trim().is_empty() {
        return Err(ApiError::Validation(
            "genre must contain at least one non-whitespace character".to_string(),
        ));
    }

    Ok(())
}
