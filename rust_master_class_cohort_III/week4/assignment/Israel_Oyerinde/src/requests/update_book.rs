use serde::Deserialize;

use super::{validate_author, validate_genre, validate_title, Validate};
use crate::error::ApiError;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateBookRequest {
    pub(crate) title: Option<String>,
    pub(crate) author: Option<String>,
    pub(crate) genre: Option<String>,
    pub(crate) available: Option<bool>,
}

impl Validate for UpdateBookRequest {
    fn validate(&self) -> Result<(), ApiError> {
        if let Some(title) = &self.title {
            validate_title(title)?;
        }
        if let Some(author) = &self.author {
            validate_author(author)?;
        }
        if let Some(genre) = &self.genre {
            validate_genre(genre)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_only_fields_that_were_supplied() {
        let available_only = UpdateBookRequest {
            title: None,
            author: None,
            genre: None,
            available: Some(false),
        };
        assert!(available_only.validate().is_ok());

        let invalid_title = UpdateBookRequest {
            title: Some(String::new()),
            author: None,
            genre: None,
            available: None,
        };
        assert!(invalid_title.validate().is_err());
    }
}
