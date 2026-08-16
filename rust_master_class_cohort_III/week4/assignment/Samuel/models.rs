use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub id: u64,
    pub title: String,
    pub author: String,
    pub genre: String,
    pub available: bool,
    pub added_at: String,
}

#[derive(Debug, Deserialize)]
pub struct BookFilters {
    pub genre: Option<String>,
    pub available: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub q: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct CreateBook {
    pub title: String,
    pub author: String,
    pub genre: String,
    pub available: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBook {
    pub title: String,
    pub author: String,
    pub genre: String,
    pub available: bool,
}

#[derive(Debug, Deserialize)]
pub struct PatchBook {
    pub title: Option<String>,
    pub author: Option<String>,
    pub genre: Option<String>,
    pub available: Option<bool>,
}
