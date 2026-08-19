use serde::Deserialize;

use super::{validate_author, validate_genre, validate_title, Validate};
use crate::error::ApiError;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceBookRequest {
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) genre: String,
    pub(crate) available: bool,
}

impl Validate for ReplaceBookRequest {
    fn validate(&self) -> Result<(), ApiError> {
        validate_title(&self.title)?;
        validate_author(&self.author)?;
        validate_genre(&self.genre)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_every_replacement_field() {
        let valid = ReplaceBookRequest {
            title: "Programming Rust".to_string(),
            author: "Jim Blandy".to_string(),
            genre: "Technical".to_string(),
            available: false,
        };
        assert!(valid.validate().is_ok());

        let invalid = ReplaceBookRequest {
            genre: " ".to_string(),
            ..valid
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn deserialization_requires_available() {
        let json = r#"{"title":"A","author":"B","genre":"C"}"#;

        assert!(serde_json::from_str::<ReplaceBookRequest>(json).is_err());
    }
}
