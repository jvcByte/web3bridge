//! Week 4 · Day 2 — Axum Fundamentals + Serde
//!
//! Five routes from the curriculum, plus a `Path` and a `Query` demo.
//!
//! Deliberate limitation: there is no shared state today, so `POST /posts`
//! cannot persist. `GET /posts` returns a seeded list. Fixing that is Day 3.
//!
//! Run with: `cargo run`

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// What we send *out*. Derives `Serialize` because it becomes JSON.
///
/// `rename_all = "camelCase"` decouples the wire format from Rust naming: the
/// field stays `created_at` in Rust and goes out as `createdAt`. This is the
/// point of serde — the JSON shape is not forced to match your struct names.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Post {
    id: u64,
    title: String,
    body: String,
    author: String,
    created_at: String,
}

/// What we accept *in*. A separate type from `Post` on purpose: the client does
/// not get to choose the `id` or the `created_at`. Modelling the request body as
/// its own struct is how you make that impossible rather than merely discouraged.
#[derive(Debug, Deserialize)]
struct CreatePost {
    title: String,
    body: String,
    #[serde(default = "anonymous")]
    author: String,
}

fn anonymous() -> String {
    "anonymous".to_string()
}

#[derive(Debug, Serialize)]
struct About {
    name: &'static str,
    version: &'static str,
    cohort: &'static str,
    framework: &'static str,
}

/// Query parameters for `/search`. Note `Option<String>` for a genuinely optional
/// field, and `#[serde(default)]` to supply a fallback rather than 422-ing.
#[derive(Debug, Deserialize)]
struct SearchParams {
    q: Option<String>,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    10
}

// ---------------------------------------------------------------------------
// Seed data
//
// Stands in for a database. Rebuilt on every request, which is exactly why
// nothing survives a POST.
// ---------------------------------------------------------------------------

fn seed_posts() -> Vec<Post> {
    vec![
        Post {
            id: 1,
            title: "Futures are lazy".into(),
            body: "Calling an async fn runs none of its body.".into(),
            author: "Ada".into(),
            created_at: "2026-08-03T09:00:00Z".into(),
        },
        Post {
            id: 2,
            title: "join! vs spawn".into(),
            body: "One task interleaved, versus N tasks needing Send + 'static.".into(),
            author: "Grace".into(),
            created_at: "2026-08-04T09:00:00Z".into(),
        },
    ]
}

// ---------------------------------------------------------------------------
// Handlers
//
// Every one of these is an ordinary `async fn`. Nothing is registered anywhere;
// axum has a blanket impl of `Handler` for async fns whose arguments are all
// extractors and whose return type is `IntoResponse`. The type system is the
// wiring.
// ---------------------------------------------------------------------------

/// Simplest possible handler: no extractors, returns `&'static str`.
/// `IntoResponse for &str` gives it a 200 and `content-type: text/plain`.
async fn root() -> &'static str {
    "Web3Bridge Blog API — Week 4 Day 2. Try /health, /about, /posts"
}

/// `Json<Value>` built by hand with the `json!` macro. Useful when the shape is
/// ad hoc and does not deserve a struct.
async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "uptime": "n/a — no state yet, that's tomorrow",
    }))
}

/// `Json<T>` where `T: Serialize`. This is the form you will use for real payloads.
async fn about() -> Json<About> {
    Json(About {
        name: "blog-api",
        version: env!("CARGO_PKG_VERSION"),
        cohort: "rust_master_class_cohort_III",
        framework: "axum 0.8 on tokio",
    })
}

async fn list_posts() -> Json<Vec<Post>> {
    Json(seed_posts())
}

/// `Path<u64>` pulls the `{id}` capture out of the URI and parses it.
///
/// Returning `Result<Json<Post>, StatusCode>` works because `IntoResponse` is
/// implemented for `Result<T, E>` when both sides are `IntoResponse`. A bare
/// `StatusCode` as the error is fine for today; Day 4 replaces it with a real
/// error type carrying a JSON body.
///
/// Hit `/posts/abc` to see axum reject the parse with a 400 before your code runs.
async fn get_post(Path(id): Path<u64>) -> Result<Json<Post>, StatusCode> {
    seed_posts()
        .into_iter()
        .find(|p| p.id == id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// `Query<T>` deserializes the query string into a struct.
async fn search(Query(params): Query<SearchParams>) -> Json<Value> {
    let needle = params.q.unwrap_or_default().to_lowercase();

    let hits: Vec<Post> = seed_posts()
        .into_iter()
        .filter(|p| {
            needle.is_empty()
                || p.title.to_lowercase().contains(&needle)
                || p.body.to_lowercase().contains(&needle)
        })
        .take(params.limit)
        .collect();

    Json(json!({
        "query": needle,
        "limit": params.limit,
        "count": hits.len(),
        "results": hits,
    }))
}

/// `Json<CreatePost>` is a `FromRequest` extractor: it consumes the body, so it
/// must be the **last** argument. Add a `Path` after it and the compiler emits a
/// trait-bound wall that only makes sense once you know this rule.
///
/// The extractor also does the validation-free rejections for you: wrong
/// content-type gives 415, malformed or incomplete JSON gives 422. Neither is
/// code you wrote.
///
/// Returning a tuple `(StatusCode, Json<T>)` overrides the default 200 — this is
/// the `IntoResponse` impl for tuples doing the work.
async fn create_post(Json(payload): Json<CreatePost>) -> (StatusCode, Json<Post>) {
    // A real id would come from the store. There is no store, so this is a lie
    // we tell for one request only.
    let created = Post {
        id: 999,
        title: payload.title,
        body: payload.body,
        author: payload.author,
        created_at: "2026-08-08T11:30:00Z".into(),
    };

    println!("created (and immediately forgot): {created:?}");

    (StatusCode::CREATED, Json(created))
}

// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    // The Router is just a value. You can build it, return it from a function,
    // nest it, and — importantly for Day 5 — test it without binding a port.
    //
    // Note the `{id}` capture syntax. Axum 0.8 dropped the older `:id` form.
    let app = Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/about", get(about))
        .route("/search", get(search))
        .route("/posts", get(list_posts).post(create_post))
        .route("/posts/{id}", get(get_post));

    let addr = "127.0.0.1:3000";
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("port 3000 already in use?");

    println!("listening on http://{addr}");
    println!();
    println!("  curl localhost:3000/health");
    println!("  curl localhost:3000/posts");
    println!("  curl 'localhost:3000/search?q=lazy'");
    println!("  curl -X POST localhost:3000/posts -H 'content-type: application/json' \\");
    println!("       -d '{{\"title\":\"Hi\",\"body\":\"There\"}}'");
    println!();
    println!("then `curl localhost:3000/posts` again and notice the new post is gone.");

    axum::serve(listener, app).await.unwrap();
}
