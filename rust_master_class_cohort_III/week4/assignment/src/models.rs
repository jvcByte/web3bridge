use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Deserialize)]
pub struct CreateBook {
    pub title: String,
    pub author: String,
    pub genre: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReplaceBook {
    pub title: String,
    pub author: String,
    pub genre: String,
    pub available: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBook {
    pub title: Option<String>,
    pub author: Option<String>,
    pub genre: Option<String>,
    pub available: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FilterParams {
    pub genre: Option<String>,
    pub available: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchParams {
    pub q: String,
    pub limit: Option<usize>,
}
