# Building a Book Library API with Axum and Tokio

This guide explains the assignment from Milestone 1 through Milestone 21. It
follows the finished implementation and focuses especially on the code that was
completed for you.

Do not memorize it. Read one section, find that section in the source, explain
it aloud, then rebuild it without looking.

## Project map

```text
src/
├── main.rs    routes, handlers, middleware, validation
├── models.rs  stored, request, and query data
├── store.rs   shared in-memory state and seed data
└── error.rs   centralized HTTP errors
```

The complete request flow is:

```text
TCP request
    ↓
request logger
    ↓
router
    ├── public route
    └── protected write route
            ↓
       API-key middleware
    ↓
extractors
    ↓
handler
    ↓
Arc<Mutex<Store>>
    ↓
Result<success, ApiError>
    ↓
HTTP response
```

---

## M1: the smallest Axum server

The first endpoint was `GET /health → "ok"`.

```rust
#[tokio::main]
async fn main() {
    let app = Router::new().route("/health", get(health));
    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

An async function creates a future: a value representing work that may finish
later. Tokio is the runtime that polls those futures. While one task waits for
network activity, Tokio can run other ready tasks.

`#[tokio::main]` generates a synchronous entry point that creates a Tokio
runtime and runs the async body inside it.

`TcpListener` owns the listening socket. `axum::serve` repeatedly accepts
connections, passes requests to the router, and writes responses back.

A router maps an HTTP method and path to a handler:

```rust
.route("/health", get(health))
```

A handler is an async function whose arguments Axum can extract from a request
and whose result Axum can convert into a response.

---

## M2: model the resource and the allowed input

The final models are in [src/models.rs](src/models.rs).

`Book` represents what is stored and returned:

```rust
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
```

- `Serialize` lets Serde turn a book into JSON.
- `Clone` lets a handler take owned data out of the locked store.
- `Debug` helps during development.
- `rename_all = "camelCase"` turns Rust's `added_at` into JSON's `addedAt`.

`CreateBook` contains only `title`, `author`, and `genre`. The server controls
`id`, `available`, and `added_at`.

This is why one giant request model would be unsafe: it would let a client send
fields it does not own.

The fields are public because the models and handlers are in different Rust
modules. A public struct with private fields could be named elsewhere but not
constructed or inspected there.

---

## M3–M4: shared state, locking, and seed data

The store is in [src/store.rs](src/store.rs):

```rust
pub struct Store {
    pub books: HashMap<u64, Book>,
    pub next_id: u64,
}

pub type SharedState = Arc<Mutex<Store>>;
```

### HashMap

`HashMap<u64, Book>` maps an ID directly to a book. It supports lookup,
insertion, and removal by ID.

### Arc

`Arc<T>` provides atomically reference-counted shared ownership. Axum can clone
the `Arc` for many concurrent requests without cloning the store itself.

```text
request A ─┐
request B ─┼── Arc ── one Store
request C ─┘
```

### Mutex

`Mutex<Store>` permits only one protected access at a time. `state.lock()?`
returns a guard that behaves like a reference to the store. Dropping the guard
unlocks the mutex.

### Why next_id shares the same mutex

Creation is one logical operation:

```text
read next_id
→ build book using that ID
→ insert book
→ increment next_id
```

Keeping the map and counter inside one lock preserves their relationship. PUT
and PATCH never increment the counter because they do not create resources.

### Why a standard Mutex is acceptable here

Every critical section is short and contains no `.await`. Holding a standard
mutex guard across `.await` can block an executor thread and can make a handler
future non-Send.

### Seed construction

`Store::seeded()` inserts IDs 1 and 2 and sets `next_id` to 3. Therefore the
first successful POST gets ID 3.

---

## M5: State and JSON extractors

```rust
async fn health(
    State(state): State<SharedState>,
) -> Result<Json<HealthResponse>, ApiError>
```

`State` extracts the shared router state. The pattern `State(state)` unwraps the
extractor.

```rust
let books = {
    let store = state.lock()?;
    store.books.len()
};
```

The braces create a scope. The guard is dropped at the closing brace, before
the response is created.

`Json<T>` serializes `T` and sets `Content-Type: application/json`. `Ok(...)`
is the successful side of the handler's `Result`.

---

## M6: list books without holding the lock too long

```rust
let mut books = {
    let store = state.lock()?;
    store.books.values().cloned().collect::<Vec<Book>>()
};

books.sort_by_key(|book| book.id);
```

`values()` yields `&Book` references. `cloned()` converts each reference into
an owned `Book`. `collect` builds a vector.

The clones matter because references into the HashMap cannot outlive the mutex
guard. Once the vector owns its books, filtering, sorting, and JSON conversion
can happen after unlocking.

A HashMap has no guaranteed ID order, so sorting is required by the API.

---

## M7: path extraction and Option

The route `/books/{id}` has a dynamic segment. `Path<u64>` parses it.

`HashMap::get` returns:

```text
Some(&Book) when the ID exists
None        when it does not
```

The final lookup is:

```rust
store
    .books
    .get(&id)
    .cloned()
    .ok_or_else(|| ApiError::NotFound(format!("book {id} not found")))?
```

`ok_or_else` converts `Option<Book>` into `Result<Book, ApiError>`. The closure
only builds the error when the book is absent.

We briefly tried returning `Option<Json<Book>>`. In this Axum version that did
not satisfy the required handler response trait, producing the vague
`Handler<_, _>` error. A temporary `Result<Json<Book>, StatusCode>` compiled;
the final `Result<Json<Book>, ApiError>` also gives the correct JSON error body.

The finished handler accepts `Result<Path<u64>, PathRejection>` so an invalid
path such as `/books/not-a-number` becomes the standard JSON 400 response.

---

## M8: centralized error handling

The error system is in [src/error.rs](src/error.rs).

```rust
pub enum ApiError {
    NotFound(String),
    ValidationFailed(String),
    Unauthorized(String),
    Conflict(String),
    Internal(String),
}
```

`thiserror::Error` implements Rust's standard error and display behavior.
`#[error("{0}")]` displays the inner message.

The required nested JSON needs two structs:

```rust
struct ErrorResponse {
    error: ErrorBody,
}

struct ErrorBody {
    kind: &'static str,
    message: String,
}
```

### IntoResponse

`IntoResponse` is the bridge between a Rust value and an HTTP response. The
implementation matches each variant to a status, stable kind, and message:

```text
NotFound         → 404, not_found
ValidationFailed → 400, validation_failed
Unauthorized     → 401, unauthorized
Conflict         → 409, conflict
Internal         → 500, internal_error
```

It delegates the final conversion:

```rust
(status, Json(body)).into_response()
```

Now handlers report meaning with `Err(ApiError::NotFound(...))` instead of
repeating response-formatting code.

### Result becomes a response

For `Result<Json<Book>, ApiError>`:

- `Ok(Json(book))` uses JSON's response conversion.
- `Err(error)` uses `ApiError::into_response`.

### The question-mark operator and From

```rust
let store = state.lock()?;
```

is conceptually:

```rust
let store = match state.lock() {
    Ok(guard) => guard,
    Err(error) => return Err(ApiError::from(error)),
};
```

`Mutex::lock` returns `PoisonError<T>`, but the handler returns `ApiError`.
This implementation provides the conversion:

```rust
impl<T> From<PoisonError<T>> for ApiError
```

The `?` operator uses `From` before returning early.

In `validate_title(title)?`, both sides already use `ApiError`, so `?` only
propagates the error; no different type is converted.

### Poisoning and safe 500s

A standard mutex is poisoned when a thread panics while holding it, warning
that protected data may be inconsistent.

The application converts poisoning to `ApiError::Internal`. The real detail is
logged with `eprintln!`, but the client receives only:

```json
{"error":{"kind":"internal_error","message":"internal server error"}}
```

That prevents secrets, paths, and implementation details from leaking.

---

## M9: POST /books

Creation has three phases.

### 1. Extract and normalize errors

The handler receives `Result<Json<CreateBook>, JsonRejection>`. The
`extract_json` helper maps malformed JSON into `ValidationFailed` so even
extractor failures follow the API's JSON error contract.

A body-consuming extractor such as JSON must be the last handler argument.
State and path extractors inspect request parts; JSON consumes the body.

### 2. Validate before locking

`validate_book_fields` checks:

- trimmed title is nonempty;
- title is at most 150 characters;
- trimmed author is nonempty;
- trimmed genre is nonempty.

Title length uses `title.chars().count()`. `String::len()` counts UTF-8 bytes,
and one non-ASCII character can occupy several bytes.

Validation runs before locking because it does not need shared data.

### 3. Mutate under one lock

While holding the store lock, the handler:

1. checks duplicate titles with `values().any(...)`;
2. reads `next_id`;
3. builds the Book;
4. inserts a clone;
5. increments `next_id`.

`any` stops at the first match.

The server assigns:

```rust
available: true,
added_at: chrono::Utc::now().to_rfc3339(),
```

Chrono is used because the standard library does not directly format RFC 3339
timestamps.

```rust
Ok((StatusCode::CREATED, Json(book)))
```

combines the required 201 status with a JSON body.

---

## M10: DELETE /books/{id}

`HashMap::remove` returns `Option<Book>`. `None` becomes NotFound. Success
returns `StatusCode::NO_CONTENT`.

HTTP 204 must have no response body, so returning only the status is correct.
Deletion does not decrease `next_id`; IDs are not reused.

---

## M11: PUT is full replacement

`ReplaceBook` requires every editable field and omits `id` and `added_at`.

PUT answers two separate questions:

1. Does the path ID exist?
2. Does another book already use the requested title?

Existence must use:

```rust
store.books.get(&id)
```

Duplicate detection uses:

```rust
.any(|book| book.id != id && book.title == payload.title)
```

`book.id != id` excludes the current book so retaining its own title does not
conflict with itself.

An earlier implementation mistakenly located the existing record by title.
That made changing a title look like a missing book and could mishandle a
nonexistent ID. Resource identity comes from the path ID, not editable content.

The replacement preserves:

```rust
id: existing_book.id,
added_at: existing_book.added_at,
```

PUT does not increment `next_id`.

---

## M12: PATCH is partial update

`UpdateBook` uses optional fields:

```rust
pub struct UpdateBook {
    pub title: Option<String>,
    pub author: Option<String>,
    pub genre: Option<String>,
    pub available: Option<bool>,
}
```

Serde maps an absent field to `None` and a supplied field to `Some(value)`.

For `{"available": false}`, availability is `Some(false)`. The correct logic
tests whether the option is supplied:

```rust
if let Some(available) = payload.available {
    book.available = available;
}
```

It must not test whether the boolean itself is true.

Validation borrows optional strings using `as_ref()`:

```rust
if let Some(title) = payload.title.as_ref() {
    validate_title(title)?;
}
```

This inspects the string without moving it. Later, the owned string can move
into the stored book.

The handler checks duplicates before calling `get_mut`. Iterating immutably
through the map while a mutable reference into that map is alive would violate
Rust's borrowing rules.

PATCH never assigns `id` or `added_at`, so both remain unchanged.

---

## M13: optional query filters

`FilterParams` contains:

```rust
genre: Option<String>
available: Option<bool>
```

Examples:

```text
/books?genre=Technical
/books?available=false
/books?genre=Technical&available=true
```

After cloning and unlocking, `retain` removes books that do not match.
Availability uses boolean equality. Genre uses `eq_ignore_ascii_case`.
The result is then sorted by ID.

Capturing `QueryRejection` ensures invalid input such as
`?available=maybe` becomes a JSON 400 rather than Axum's default plain text.

---

## M14: case-insensitive search and limits

`SearchParams` requires `q` and optionally accepts `limit`.

The query and candidate strings are lowercased. A book matches if its title or
author contains the query.

```rust
let limit = params.limit.unwrap_or(10);
books.sort_by_key(|book| book.id);
books.truncate(limit);
```

`unwrap_or(10)` supplies the default. `truncate` keeps at most the requested
number of results.

This is appropriate for a small in-memory store. A real database service would
normally filter, sort, and limit inside the database query.

---

## M15: API-key middleware

Authentication is applied around write handlers instead of copied into them.

```text
public routes:    GET health/books/search
protected routes: POST/PUT/PATCH/DELETE
```

Only `write_routes` receives `route_layer(require_api_key)`.

The expected key is read once at startup:

```rust
env::var("API_KEY").unwrap_or_else(|_| "dev-secret-key".to_string())
```

Middleware receives a `Request` and `Next`. It either returns early with 401 or
continues with:

```rust
next.run(request).await
```

The comparison helper does not stop at the first different byte. It accumulates
all byte and length differences and succeeds only when the final difference is
zero.

For real cryptographic systems, use a reviewed constant-time library.
Handwritten timing-sensitive code can be affected by compiler and platform
behavior.

---

## M16: request logging with an atomic counter

The counter is `Arc<AtomicU64>`. Each request uses:

```rust
counter.fetch_add(1, Ordering::Relaxed) + 1
```

The atomic makes incrementing indivisible without a mutex. `Relaxed` ordering
is enough because the number does not coordinate access to any other memory; it
only needs to be unique.

The logger captures method, path, and `Instant::now()` before calling the next
service. After `next.run(request).await`, it knows the status and elapsed time.

Code before `next.run` runs on the way into the handler. Code after it runs on
the way back out.

### Layer order

The logger is added after public and protected routers are merged and after the
fallback is registered:

```text
public + protected + fallback
            ↓
          logger
```

Axum layers apply to routes already present. Adding the logger before merging
write routes would leave those later routes, including their 401 responses,
outside the logging layer.

---

## M17: unknown-route JSON 404

`fallback(route_not_found)` handles paths that match no route. It returns an
`ApiError::NotFound` containing the path.

Using the shared error enum guarantees the same content type and nested JSON
shape as a missing book.

---

## M18: extractor errors and internal safety

Axum can reject input before normal application logic:

- malformed JSON;
- a nonnumeric path ID;
- an invalid query boolean;
- a missing required search query.

The handlers request these as:

```rust
Result<Json<T>, JsonRejection>
Result<Path<u64>, PathRejection>
Result<Query<T>, QueryRejection>
```

`extract_json`, `extract_path`, and `extract_query` convert them to
`ValidationFailed`. Therefore every 400 produced by these paths has the normal
JSON error shape.

Internal diagnostics are kept server-side while clients receive a generic 500
message. Stable public errors should not expose unstable implementation details.

---

## M19: the acceptance script

[check.sh](check.sh) treats the server as an external client. It verifies public
reads, error shape, auth, validation, CRUD lifecycle, PATCH preservation,
`available: false`, conflicts, filters, and search.

```bash
curl -s -o /dev/null -w '%{http_code}'
```

lets the script assert status codes. It exits at the first failure so one root
problem does not create a wall of misleading later failures.

Additional manual checks covered filter contents, author search, limits, PATCH
conflicts, missing IDs, malformed extractors, custom `API_KEY` values, and
logger coverage.

---

## M20: final architecture

- [src/models.rs](src/models.rs) owns API data shapes.
- [src/store.rs](src/store.rs) owns shared data and seed construction.
- [src/error.rs](src/error.rs) owns error categories and HTTP conversion.
- [src/main.rs](src/main.rs) owns wiring, handlers, middleware, and validation.

A larger service could split handlers and middleware further. For this
assignment, keeping the complete request flow visible in `main.rs` is easier to
learn from.

---

## M21: formatting, verification, Git, and PR

The final commands were:

```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
bash check.sh
```

- `cargo fmt --check` proves standard formatting.
- `cargo check` proves the types and trait bounds compile.
- `cargo test` compiles test targets and runs defined Rust tests.
- `cargo clippy -D warnings` treats all Clippy warnings as failures.
- `check.sh` verifies the external HTTP contract.

`Cargo.lock` records exact dependency versions for reproducible executable
builds. `.gitignore` prevents `target/` build artifacts from being committed.

The assignment was committed on `week4-assignment`, pushed to the fork, and
opened as a draft PR against upstream `main`.

---

## Ownership and borrowing patterns used repeatedly

### Borrow, clone, release

```text
borrow Store through mutex guard
→ clone needed data
→ drop guard
→ build response from owned data
```

This prevents responses from borrowing locked data.

### Borrow before moving

PATCH uses `as_ref()` to validate and compare optional strings, then later moves
the owned values into the book.

### Why writes clone Book

The HashMap needs ownership of one book, while the response also needs an owned
book. Inserting `book.clone()` lets the map own the clone and the handler return
the original.

---

## Three complete request journeys

### GET /books?available=false

```text
logger starts
→ public router matches
→ Query parses false
→ handler locks Store
→ handler clones books
→ mutex unlocks
→ vector filters and sorts
→ Json serializes
→ logger records 200 and elapsed time
→ client receives response
```

### POST /books

```text
logger starts
→ write router matches
→ API-key middleware authenticates
→ Json parses CreateBook
→ validation runs
→ Store locks
→ duplicate check
→ ID allocation, insert, increment
→ mutex unlocks
→ handler returns 201 JSON
→ logger records result
```

If authentication fails, the handler never runs, but the outer logger still
records 401.

### GET /books/999

```text
logger starts
→ Path parses 999
→ Store locks
→ HashMap returns None
→ ok_or_else creates NotFound
→ ? returns early
→ IntoResponse creates 404 JSON
→ logger records 404
```

---

## Limits of this learning project

The server has no persistent database, graceful shutdown, structured tracing,
rate limiting, API-key rotation, Unicode-aware case folding, OpenAPI document,
or substantial Rust unit-test suite. Its in-memory state resets on restart.

Those are sensible next steps, but they are separate from the concepts this
assignment was designed to teach.

---

## Reconstruction exercises

1. Rebuild `health` and explain exactly when the mutex unlocks.
2. Find every `?` and state whether `From` converts its error.
3. Break PUT by looking up the existing book by title; predict each failure.
4. Trace all four `UpdateBook` fields for `{"available": false}`.
5. Draw middleware order for a 200 GET, 201 POST, 401 POST, and fallback 404.
6. Add unit tests for validation and constant-time comparison.
7. Replace the HashMap with a database and list which API layers stay unchanged.

## Final self-check

You understand the assignment when you can explain:

1. What Tokio does and why `main` is async.
2. How Axum routes, extractors, handlers, and responses connect.
3. Why each request model differs from `Book`.
4. Why state is `Arc<Mutex<Store>>`.
5. Why `next_id` is inside the same mutex.
6. Why handlers clone before unlocking.
7. How `Option` becomes NotFound.
8. How `IntoResponse` centralizes errors.
9. Where `?` invokes `From`.
10. Why JSON must be the final body-consuming extractor.
11. How PUT differs from PATCH.
12. Why `Some(false)` must be handled.
13. Why duplicate checking precedes `get_mut`.
14. Why only write routes receive auth middleware.
15. Why the logger uses `AtomicU64` with `Relaxed`.
16. Why logger placement after `merge` matters.
17. Why internal details stay out of 500 responses.
18. What each final verification command proves.

If an answer is vague, rebuild that milestone in isolation.
