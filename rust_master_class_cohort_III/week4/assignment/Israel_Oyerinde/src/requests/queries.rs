use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct ListBooksQuery {
    pub(crate) genre: Option<String>,
    pub(crate) available: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SearchBooksQuery {
    pub(crate) q: Option<String>,
    pub(crate) limit: Option<usize>,
}
