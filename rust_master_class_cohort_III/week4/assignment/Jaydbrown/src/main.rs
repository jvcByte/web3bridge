use axum::{
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BookDetails {
    id: u64,
    title: String,
    author: String,
    genre: String,
    available: bool,
    added_at: String,
}

#[derive(Debug, Deserialize)]
struct NewBook {
    title: String,
    author: String,
    genre: String,
}

#[derive(Debug, Deserialize)]
struct ReplaceBook {
    title: String,
    author: String,
    genre: String,
    available: bool,
}

#[derive(Debug, Deserialize)]
struct UpdateBook {
    title: Option<String>,
    author: Option<String>,
    genre: Option<String>,
    available: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct FilterParams {
    genre: Option<String>,
    available: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    q: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, thiserror::Error)]
enum ApiError {
    #[error("book {0} not found")]
    NotFound(u64),
    #[error("route not found")]
    RouteNotFound,
    #[error("{0}")]
    Validation(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("{0}")]
    Conflict(String),
    #[error("internal error")]
    Internal(String),
}

impl<T> From<std::sync::PoisonError<T>> for ApiError {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        ApiError::Internal("shared state lock was poisoned".to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, kind, message) = match self {
            ApiError::NotFound(id) => (
                StatusCode::NOT_FOUND,
                "not_found",
                format!("book {id} not found"),
            ),
            ApiError::RouteNotFound => (
                StatusCode::NOT_FOUND,
                "not_found",
                "route not found".to_string(),
            ),
            ApiError::Validation(message) => (StatusCode::BAD_REQUEST, "validation_failed", message),
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "missing or invalid API key".to_string(),
            ),
            ApiError::Conflict(message) => (StatusCode::CONFLICT, "conflict", message),
            ApiError::Internal(message) => {
                eprintln!("internal error: {message}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "internal server error".to_string(),
                )
            }
        };

        let body = Json(json!({
            "error": { "kind": kind, "message": message }
        }));
        (status, body).into_response()
    }
}

fn validate_title(title: &str) -> Result<(), ApiError> {
    if title.trim().is_empty() {
        return Err(ApiError::Validation("title must not be empty".to_string()));
    }
    if title.chars().count() > 150 {
        return Err(ApiError::Validation(
            "title must be at most 150 characters".to_string(),
        ));
    }
    Ok(())
}

fn validate_nonempty(field: &str, value: &str) -> Result<(), ApiError> {
    if value.trim().is_empty() {
        return Err(ApiError::Validation(format!("{field} must not be empty")));
    }
    Ok(())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn now_rfc3339() -> String {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = since_epoch.as_secs();
    let millis = since_epoch.subsec_millis();
    let days = (secs / 86_400) as i64;
    let time_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}

struct Library {
    books: Vec<BookDetails>,
    next_id: u64,
}

impl Library {
    fn seed() -> Self {
        Library {
            books: vec![
                BookDetails {
                    id: 1,
                    title: "The Rust Programming Language".to_string(),
                    author: "Steve Klabnik".to_string(),
                    genre: "Technical".to_string(),
                    available: true,
                    added_at: now_rfc3339(),
                },
                BookDetails {
                    id: 2,
                    title: "Programming Rust".to_string(),
                    author: "Jim Blandy".to_string(),
                    genre: "Technical".to_string(),
                    available: false,
                    added_at: now_rfc3339(),
                },
            ],
            next_id: 3,
        }
    }

    fn add_book(&mut self, new_book: NewBook) -> Result<BookDetails, ApiError> {
        validate_title(&new_book.title)?;
        validate_nonempty("author", &new_book.author)?;
        validate_nonempty("genre", &new_book.genre)?;
        if self.books.iter().any(|b| b.title == new_book.title) {
            return Err(ApiError::Conflict(format!(
                "a book titled '{}' already exists",
                new_book.title
            )));
        }

        let id = self.next_id;
        self.next_id += 1;
        let book = BookDetails {
            id,
            title: new_book.title,
            author: new_book.author,
            genre: new_book.genre,
            available: true,
            added_at: now_rfc3339(),
        };
        self.books.push(book.clone());
        Ok(book)
    }

    fn list_books(&self, filter: &FilterParams) -> Vec<BookDetails> {
        let mut books: Vec<BookDetails> = self
            .books
            .iter()
            .filter(|b| filter.genre.as_ref().map_or(true, |g| &b.genre == g))
            .filter(|b| filter.available.map_or(true, |a| b.available == a))
            .cloned()
            .collect();
        books.sort_by_key(|b| b.id);
        books
    }

    fn get_book(&self, id: u64) -> Option<BookDetails> {
        self.books.iter().find(|b| b.id == id).cloned()
    }

    fn search_books(&self, params: &SearchParams) -> Vec<BookDetails> {
        let q = params.q.clone().unwrap_or_default().to_lowercase();
        let limit = params.limit.unwrap_or(10);
        let mut books: Vec<BookDetails> = self
            .books
            .iter()
            .filter(|b| b.title.to_lowercase().contains(&q) || b.author.to_lowercase().contains(&q))
            .cloned()
            .collect();
        books.sort_by_key(|b| b.id);
        books.truncate(limit);
        books
    }

    fn replace_book(&mut self, id: u64, replacement: ReplaceBook) -> Result<BookDetails, ApiError> {
        validate_title(&replacement.title)?;
        validate_nonempty("author", &replacement.author)?;
        validate_nonempty("genre", &replacement.genre)?;
        if self
            .books
            .iter()
            .any(|b| b.id != id && b.title == replacement.title)
        {
            return Err(ApiError::Conflict(format!(
                "a book titled '{}' already exists",
                replacement.title
            )));
        }

        let book = self
            .books
            .iter_mut()
            .find(|b| b.id == id)
            .ok_or(ApiError::NotFound(id))?;
        book.title = replacement.title;
        book.author = replacement.author;
        book.genre = replacement.genre;
        book.available = replacement.available;
        Ok(book.clone())
    }

    fn update_book(&mut self, id: u64, update: UpdateBook) -> Result<BookDetails, ApiError> {
        if let Some(title) = &update.title {
            validate_title(title)?;
        }
        if let Some(author) = &update.author {
            validate_nonempty("author", author)?;
        }
        if let Some(genre) = &update.genre {
            validate_nonempty("genre", genre)?;
        }
        if let Some(title) = &update.title {
            if self.books.iter().any(|b| b.id != id && &b.title == title) {
                return Err(ApiError::Conflict(format!(
                    "a book titled '{title}' already exists"
                )));
            }
        }

        let book = self
            .books
            .iter_mut()
            .find(|b| b.id == id)
            .ok_or(ApiError::NotFound(id))?;
        if let Some(title) = update.title {
            book.title = title;
        }
        if let Some(author) = update.author {
            book.author = author;
        }
        if let Some(genre) = update.genre {
            book.genre = genre;
        }
        if let Some(available) = update.available {
            book.available = available;
        }
        Ok(book.clone())
    }

    fn delete_book(&mut self, id: u64) -> Result<(), ApiError> {
        let pos = self
            .books
            .iter()
            .position(|b| b.id == id)
            .ok_or(ApiError::NotFound(id))?;
        self.books.remove(pos);
        Ok(())
    }
}

type SharedLibrary = Arc<Mutex<Library>>;

async fn create_book(
    State(library): State<SharedLibrary>,
    Json(new_book): Json<NewBook>,
) -> Result<(StatusCode, Json<BookDetails>), ApiError> {
    let mut library = library.lock()?;
    let book = library.add_book(new_book)?;
    Ok((StatusCode::CREATED, Json(book)))
}

async fn list_books(
    State(library): State<SharedLibrary>,
    Query(filter): Query<FilterParams>,
) -> Result<Json<Vec<BookDetails>>, ApiError> {
    let library = library.lock()?;
    Ok(Json(library.list_books(&filter)))
}

async fn get_book(
    State(library): State<SharedLibrary>,
    Path(id): Path<u64>,
) -> Result<Json<BookDetails>, ApiError> {
    let library = library.lock()?;
    library.get_book(id).map(Json).ok_or(ApiError::NotFound(id))
}

async fn search_books(
    State(library): State<SharedLibrary>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<BookDetails>>, ApiError> {
    let library = library.lock()?;
    Ok(Json(library.search_books(&params)))
}

async fn health(State(library): State<SharedLibrary>) -> Result<Json<serde_json::Value>, ApiError> {
    let library = library.lock()?;
    Ok(Json(json!({ "status": "ok", "books": library.books.len() })))
}

async fn replace_book(
    State(library): State<SharedLibrary>,
    Path(id): Path<u64>,
    Json(replacement): Json<ReplaceBook>,
) -> Result<Json<BookDetails>, ApiError> {
    let mut library = library.lock()?;
    let book = library.replace_book(id, replacement)?;
    Ok(Json(book))
}

async fn update_book(
    State(library): State<SharedLibrary>,
    Path(id): Path<u64>,
    Json(update): Json<UpdateBook>,
) -> Result<Json<BookDetails>, ApiError> {
    let mut library = library.lock()?;
    let book = library.update_book(id, update)?;
    Ok(Json(book))
}

async fn delete_book(
    State(library): State<SharedLibrary>,
    Path(id): Path<u64>,
) -> Result<StatusCode, ApiError> {
    let mut library = library.lock()?;
    library.delete_book(id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn fallback_handler() -> ApiError {
    ApiError::RouteNotFound
}

async fn require_api_key(headers: HeaderMap, req: Request, next: Next) -> Result<Response, ApiError> {
    let expected = std::env::var("API_KEY").unwrap_or_else(|_| "dev-secret-key".to_string());
    let provided = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
        Ok(next.run(req).await)
    } else {
        Err(ApiError::Unauthorized)
    }
}

async fn log_requests(
    State(counter): State<Arc<AtomicU64>>,
    req: Request,
    next: Next,
) -> Response {
    let start = Instant::now();
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let n = counter.fetch_add(1, Ordering::SeqCst) + 1;

    let response = next.run(req).await;

    let elapsed = start.elapsed();
    println!(
        "[req {n:>4}] {:<7}{:<25} -> {} ({:.2}ms)",
        method.as_str(),
        path,
        response.status().as_u16(),
        elapsed.as_secs_f64() * 1000.0
    );

    response
}

#[tokio::main]
async fn main() {
    let library: SharedLibrary = Arc::new(Mutex::new(Library::seed()));
    let request_counter = Arc::new(AtomicU64::new(0));

    let public_routes = Router::new()
        .route("/books", get(list_books))
        .route("/books/{id}", get(get_book))
        .route("/search", get(search_books))
        .route("/health", get(health));

    let write_routes = Router::new()
        .route("/books", post(create_book))
        .route(
            "/books/{id}",
            put(replace_book).patch(update_book).delete(delete_book),
        )
        .route_layer(middleware::from_fn(require_api_key));

    let app = public_routes
        .merge(write_routes)
        .fallback(fallback_handler)
        .layer(middleware::from_fn_with_state(request_counter, log_requests))
        .with_state(library);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
