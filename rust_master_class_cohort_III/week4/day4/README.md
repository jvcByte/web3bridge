# Week 4 · Day 4 — Error Handling & Middleware

> Phase Two: Applied Rust & Systems Engineering
> Master curriculum: [`Phase_Two_Daily_Curriculum_Weeks_4-6.md`](../../../Phase_Two_Daily_Curriculum_Weeks_4-6.md)

---

## From the master curriculum

**Day 4 — Error Handling & Middleware**
- Pre-class: read `thiserror` docs + Axum middleware docs.
- Topics: `Result`, custom error types, `IntoResponse`, `StatusCode`, `thiserror`; logging middleware, request extensions, simple `X-API-KEY` authentication.
- Build: proper `404`/`400`/`500` responses, plus an `X-API-KEY` auth middleware layer.
- Resources:
  - thiserror: https://docs.rs/thiserror
  - Axum middleware docs: https://docs.rs/axum/latest/axum/middleware/index.html

---

## Session shape (11:00 AM – 2:00 PM)

| Time | Block |
|---|---|
| 11:00–11:25 | Concept — errors as values; what a `Layer` actually wraps |
| 11:25–12:10 | Live coding — `ApiError` + `IntoResponse`, then `from_fn` middleware |
| 12:10–12:25 | Break |
| 12:25–1:35 | Student implementation — error type, validation, `X-API-KEY` layer |
| 1:35–2:00 | Code review + debugging |

**Start with the Day 3 homework check:** `PUT` and `DELETE` working.

---

## What's in this folder

```
day4/
├── Cargo.toml
└── src/
    └── main.rs
```

Two things get added to yesterday's CRUD API.

**1. A real error type.** `ApiError` is a `thiserror` enum that implements `IntoResponse`, so
every handler returns `Result<T, ApiError>` and every failure path produces a consistent JSON
body instead of a bare status code:

```json
{ "error": { "kind": "not_found", "message": "post 42 not found" } }
```

| Variant | Status |
|---|---|
| `NotFound` | 404 |
| `Validation` | 400 |
| `Unauthorized` | 401 |
| `Internal` | 500 |

**2. Two middleware layers.**

| Layer | Applies to | Does |
|---|---|---|
| `log_requests` | every route | logs method, path, status, duration; injects a request id |
| `require_api_key` | mutating routes only | rejects requests without a valid `X-API-KEY` with 401 |

The auth layer is applied to a *nested* router holding the write routes, so `GET` stays public
while `POST`/`PUT`/`PATCH`/`DELETE` require the key. That asymmetry is the interesting part of
the routing today.

The request id is stored in **request extensions** by the logging layer and read back out by
handlers — this is the mechanism for passing data from middleware to handler, and it is how
real auth middleware hands the authenticated user down.

---

## Running it

```bash
cargo run
```

The API key is read from `API_KEY`, defaulting to `dev-secret-key`:

```bash
API_KEY=hunter2 cargo run
```

```bash
# reads are public
curl localhost:3000/posts

# writes without a key -> 401
curl -i -X POST localhost:3000/posts \
  -H 'content-type: application/json' \
  -d '{"title":"nope","body":"no key"}'

# with the key -> 201
curl -i -X POST localhost:3000/posts \
  -H 'content-type: application/json' \
  -H 'x-api-key: dev-secret-key' \
  -d '{"title":"Errors as values","body":"IntoResponse does the work"}'

# validation -> 400 with a useful message
curl -i -X POST localhost:3000/posts \
  -H 'content-type: application/json' \
  -H 'x-api-key: dev-secret-key' \
  -d '{"title":"","body":"empty title"}'

# 404 with a JSON body, not a bare status
curl -i localhost:3000/posts/999

# 500, deliberately, to show the log/response split
curl -i localhost:3000/boom

# unknown route -> 404 via the fallback
curl -i localhost:3000/nope
```

Watch the server console while you do this: every line shows `method path -> status (duration)`
with the request id.

---

## Concept notes for the 11:00 block

**Errors are values, and the response is a `From` impl away.** The pattern is: one error enum
for the whole app, `#[from]` conversions for the error types you import, and a single
`IntoResponse` impl that decides the status code. Once that exists, `?` works everywhere and
handlers stop containing error-formatting code. Show them a handler before and after — the
`match` ladders disappear.

**Why `thiserror` and not `anyhow`.** `thiserror` generates `Display` and `From` for an enum
*you* define, so callers can match on the variant — right for a library or for anything whose
errors are part of its contract, which includes an HTTP API mapping variants to status codes.
`anyhow` gives one opaque type that is easy to bubble but impossible to branch on — right for
an application binary that only reports. An API needs both halves: `thiserror` for the enum,
and `anyhow`-style opacity for the 500 case. Note how `ApiError::Internal` deliberately does
*not* leak its inner message to the client.

**Never leak internals in a 500.** Look at the `IntoResponse` impl: for `Internal` it logs the
real error server-side and returns a generic "internal server error" to the client. Database
errors and file paths in a response body are an information-disclosure bug, and it is the kind
of thing that ships when error handling is an afterthought. This is the security habit of the
day.

**What a `Layer` actually is.** Middleware in axum is `tower::Layer` — a decorator that wraps
one `Service` in another. `Service` is roughly `async fn(Request) -> Response`. So a stack of
middleware is a set of nested function calls around your handler, and `next.run(req).await` is
literally "call the inner one". Once that clicks, the whole tower ecosystem is available to
them, and Week 6 Day 4 — where they implement `Layer` and `Service` by hand for the rate
limiter — becomes a small step instead of a new topic.

`middleware::from_fn` is a convenience that turns an `async fn(Request, Next) -> Response` into
a `Layer`. Start there today; the manual impl comes in Week 6.

**Layer ordering is inside-out.** This trips everyone. Layers are applied bottom-up: the
*last* `.layer()` call in the chain is the *outermost*, so it sees the request first and the
response last. In `main.rs` the logging layer is added last precisely so it wraps everything
and can time the auth rejections too. Have them reorder it and watch 401s stop being logged.

**Middleware can short-circuit.** `require_api_key` either calls `next.run(req).await` or
returns a response without ever calling the handler. That is the whole mechanism of auth,
rate limiting, and caching. Nothing more elaborate is going on.

**Request extensions are the middleware-to-handler channel.** A typed map on the request:
`req.extensions_mut().insert(value)`, then `Extension(value)` as a handler argument. The catch
is that it is checked at runtime, not compile time — forget the middleware and the extractor
fails with a 500. Show that failure once so they recognise it.

**Constant-time comparison.** The key check uses a byte-by-byte fold, not `==`. Short-circuiting
string comparison leaks the length of the matching prefix through timing. For a class demo this
is arguably paranoid; the habit of noticing it is not.

---

## Talking points during code review (1:35)

- Where does the `?` operator's conversion happen in `create_post`? Which trait?
- Why does the 500 response body differ from what the server logs?
- Reorder the two `.layer()` calls. What changes? Why?
- What happens if you request `Extension<RequestId>` on a route the logging layer does not
  cover?
- Why is the auth layer on a nested router instead of on the whole app?
- Is `Validation` a 400 or a 422? What is axum's own `Json` rejection using, and why is ours
  different?

---

## Homework (standing)

- Read the official docs for today's topic — `axum::middleware`, `thiserror`.
- Read one crate's source for 15–20 minutes. Today: `tower::Layer` — it is about 20 lines.
- Solve or review one Rustlings exercise.
- Refactor yesterday's code.
- Write at least one unit test.

Today specifically: add a `Conflict` variant returning 409 when a post title already exists.
Then add a `tower_http::trace::TraceLayer` alongside the hand-written logger and compare what
each gives you.
