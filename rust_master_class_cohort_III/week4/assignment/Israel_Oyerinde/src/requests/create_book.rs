use serde::Deserialize;

use super::{validate_author, validate_genre, validate_title, Validate};
use crate::error::ApiError;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBookRequest {
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) genre: String,
    pub(crate) available: Option<bool>,
}

impl Validate for CreateBookRequest {
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

    fn valid_request() -> CreateBookRequest {
        CreateBookRequest {
            title: "Clean Code".to_string(),
            author: "Robert C. Martin".to_string(),
            genre: "Technical".to_string(),
            available: None,
        }
    }

    #[test]
    fn validates_all_required_creation_fields() {
        assert!(valid_request().validate().is_ok());

        let mut request = valid_request();
        request.title = " ".to_string();
        assert!(request.validate().is_err());

        let mut request = valid_request();
        request.author = String::new();
        assert!(request.validate().is_err());

        let mut request = valid_request();
        request.genre = "\t".to_string();
        assert!(request.validate().is_err());
    }

    #[test]
    fn enforces_the_title_length_limit() {
        let mut request = valid_request();
        request.title = "x".repeat(150);
        assert!(request.validate().is_ok());

        request.title.push('x');
        assert!(request.validate().is_err());
    }
}
