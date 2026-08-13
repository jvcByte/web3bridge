# Week 4 Assignment — Book Library API

> Covers: Days 1–4 · Async Rust · Axum · Serde · Shared State · Error Handling · Middleware

---

## The Brief

You are building the backend for a small digital library system.
The librarian's tool manages the collection — adding, editing, and removing books.
Anyone with the API URL can browse and search the catalogue.

Build a single Axum server that satisfies every requirement below.
You have seen every concept needed across Days 1–4.
Your only reference material is the official docs.

---

## What to Build

### The `Book` resource

Every book in the system has:

| Field | Type | Notes |
|---|---|---|
| `id` | `u64` | Assigned by the server. Client never supplies this. |
| `title` | `String` | Required. 1–150 characters. |
| `author` | `String` | Required. Must not be empty. |
| `genre` | `String` | Required. Must not be empty. |
| `available` | `bool` | Whether the book is on the shelf. Defaults to `true` on creation. |
| `added_at` | `String` | RFC 3339 timestamp. Server-assigned at creation. |

The JSON keys must use `camelCase` on the wire (`addedAt`, not `added_at`).

---

## Routes

### Public — no key required

| Method | Path | Success | Notes |
|---|---|---|---|
| `GET` | `/books` | 200 | Returns all books, sorted by `id`. Supports `?genre=` filter and `?available=true/false` filter. |
| `GET` | `/books/{id}` | 200 | Returns one book. 404 if not found. |
| `GET` | `/search` | 200 | `?q=` searches `title` and `author` (case-insensitive). `?limit=` caps results (default 10). |
| `GET` | `/health` | 200 | Returns `{"status":"ok","books":<count>}`. |

### Protected — require `X-API-KEY` header

| Method | Path | Success | Notes |
|---|---|---|---|
| `POST` | `/books` | 201 | Creates a book. Validates all fields. |
| `PUT` | `/books/{id}` | 200 | Full replacement. All fields required. Preserves `id` and `added_at`. |
| `PATCH` | `/books/{id}` | 200 | Partial update. Only supplied fields change. |
| `DELETE` | `/books/{id}` | 204 | Removes the book. |

---

## Error Shape

**Every error response — 400, 401, 404, 409, 500 — must be JSON with this exact shape:**

```json
{
  "error": {
    "kind": "not_found",
    "message": "book 42 not found"
  }
}
```

`kind` must be a stable machine-readable slug. Use these:

| Situation | `kind` |
|---|---|
| Resource not found | `"not_found"` |
| Validation failure | `"validation_failed"` |
| Missing or wrong API key | `"unauthorized"` |
| Duplicate title | `"conflict"` |
| Catch-all server error | `"internal_error"` |

**500 responses must never include internal details** — log the real error to the console, return a generic message to the client.

Unknown routes must also return a JSON 404, not an empty body.

---

## Validation Rules

On `POST` and `PUT`:
- `title` must not be empty and must be ≤ 150 characters
- `author` must not be empty
- `genre` must not be empty
- Two books cannot have the same `title` — return a `409 Conflict` with `kind: "conflict"`

On `PATCH`:
- Validate only the fields that were supplied
- `available` may be set to `false` (marking a book as checked out) — this is valid

---

## Middleware Requirements

### 1. Request Logger (all routes)
Log every request to stdout in this format:
```
[req    1] GET    /books                   -> 200 (1.23ms)
[req    2] POST   /books                   -> 201 (0.45ms)
[req    3] POST   /books                   -> 401 (0.12ms)
```
Include: request number, method, path, response status, elapsed time.

The request number must come from a **shared atomic counter** — not a Mutex.

### 2. API Key Auth (write routes only)
- Read the expected key from the `API_KEY` environment variable
- Default to `"dev-secret-key"` if not set
- Reject requests to `POST`/`PUT`/`PATCH`/`DELETE` without the correct key with `401`
- `GET` routes must remain fully public
- Use **constant-time comparison** — not `==`

---

## Technical Requirements

These are not hints — they are requirements.

- `Arc<Mutex<T>>` for shared state. The id counter must live **inside** the same mutex as the book map — not in a separate lock.
- Use `thiserror` to define your error enum. Your `IntoResponse` impl handles all HTTP mapping in one place.
- Every handler returns `Result<T, ApiError>`. No handler contains response-formatting code.
- The `PATCH` body uses `Option<T>` fields — `None` means "not supplied", not "set to null".
- `PUT` and `PATCH` must **not** allow the client to change `id` or `added_at`.

---

## Seed Data

Start the server with these two books already in the store (no need to POST them):

```
id: 1
title: "The Rust Programming Language"
author: "Steve Klabnik"
genre: "Technical"
available: true

id: 2
title: "Programming Rust"
author: "Jim Blandy"
genre: "Technical"
available: false
```

`next_id` starts at `3`.

---

## Acceptance Script

Save this as `check.sh` and run it against your server (`bash check.sh`).
It exits non-zero on the first failure.

```bash
#!/usr/bin/env bash
set -euo pipefail

H=localhost:3000
K=dev-secret-key

check() {
  [ "$1" = "$2" ] && echo "  ok   $3" || { echo "  FAIL $3: expected '$1' got '$2'"; exit 1; }
}
code()  { curl -s -o /dev/null -w '%{http_code}' "$@"; }
body()  { curl -s "$@"; }
field() { body "$@" | grep -o "\"$1\":\"[^\"]*\"" | head -1 | cut -d'"' -f4; }

echo "── public reads ──────────────────────────────────────"
check 200 "$(code $H/books)"               "GET /books"
check 200 "$(code $H/books/1)"             "GET /books/1"
check 200 "$(code $H/health)"              "GET /health"
check 404 "$(code $H/books/999)"           "GET missing book → 404"
check 404 "$(code $H/no-such-route)"       "unknown route → 404"

echo "── error shape ───────────────────────────────────────"
KIND=$(body $H/books/999 | grep -o '"kind":"[^"]*"' | cut -d'"' -f4)
check "not_found" "$KIND"                  "404 carries kind: not_found"

echo "── auth ──────────────────────────────────────────────"
check 401 "$(code -X POST $H/books \
  -H 'content-type: application/json' \
  -d '{"title":"x","author":"y","genre":"z"}')"   "POST without key → 401"

check 401 "$(code -X POST $H/books \
  -H 'content-type: application/json' \
  -H 'x-api-key: wrong' \
  -d '{"title":"x","author":"y","genre":"z"}')"   "POST wrong key → 401"

echo "── validation ────────────────────────────────────────"
check 400 "$(code -X POST $H/books \
  -H 'content-type: application/json' \
  -H "x-api-key: $K" \
  -d '{"title":"","author":"y","genre":"z"}')"    "empty title → 400"

check 400 "$(code -X POST $H/books \
  -H 'content-type: application/json' \
  -H "x-api-key: $K" \
  -d '{"title":"ok","author":"","genre":"z"}')"   "empty author → 400"

echo "── lifecycle ─────────────────────────────────────────"
check 201 "$(code -X POST $H/books \
  -H 'content-type: application/json' \
  -H "x-api-key: $K" \
  -d '{"title":"Clean Code","author":"Robert Martin","genre":"Technical"}')" \
  "POST → 201"

# GET the new book
NEW_ID=$(body "$H/books" | grep -o '"id":[0-9]*' | tail -1 | cut -d: -f2)
check 200 "$(code $H/books/$NEW_ID)"       "GET newly created book"

# PATCH — only title
check 200 "$(code -X PATCH $H/books/$NEW_ID \
  -H 'content-type: application/json' \
  -H "x-api-key: $K" \
  -d '{"title":"Clean Code 2nd Ed"}')"     "PATCH title → 200"

AUTHOR=$(field "author" $H/books/$NEW_ID)
check "Robert Martin" "$AUTHOR"            "PATCH left author untouched"

# Mark as unavailable
check 200 "$(code -X PATCH $H/books/$NEW_ID \
  -H 'content-type: application/json' \
  -H "x-api-key: $K" \
  -d '{"available":false}')"               "PATCH available:false → 200"

# PUT — full replace
check 200 "$(code -X PUT $H/books/$NEW_ID \
  -H 'content-type: application/json' \
  -H "x-api-key: $K" \
  -d '{"title":"Clean Code 2nd Ed","author":"R. C. Martin","genre":"Technical","available":true}')" \
  "PUT → 200"

# DELETE
check 204 "$(code -X DELETE $H/books/$NEW_ID -H "x-api-key: $K")" \
  "DELETE → 204"
check 404 "$(code $H/books/$NEW_ID)"       "deleted book → 404"

echo "── duplicate title ───────────────────────────────────"
check 409 "$(code -X POST $H/books \
  -H 'content-type: application/json' \
  -H "x-api-key: $K" \
  -d '{"title":"The Rust Programming Language","author":"anyone","genre":"Technical"}')" \
  "duplicate title → 409"

echo "── filters ───────────────────────────────────────────"
check 200 "$(code "$H/books?genre=Technical")"       "?genre= filter"
check 200 "$(code "$H/books?available=false")"       "?available= filter"
check 200 "$(code "$H/search?q=rust")"               "?q= search"
check 200 "$(code "$H/search?q=rust&limit=1")"       "?q= with ?limit="

echo "── 500 does not leak internals ───────────────────────"
# Trigger by requesting a route that calls an internal error path if you add one,
# or verify manually that your Internal variant never exposes raw error strings
echo "  (verify manually: your 500 response body must not contain passwords or paths)"

echo ""
echo "PASS ✓"
```

---

## Cargo.toml

```toml
[package]
name = "week4-assignment"
version = "0.1.0"
edition = "2021"

[dependencies]
axum      = "0.8"
tokio     = { version = "1", features = ["full"] }
serde     = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
```

---

## Hints

These are here if you are genuinely stuck. Try the docs first.

<details>
<summary>Where does the id counter live?</summary>

Inside the same `Mutex` as the book map — not in a separate `AtomicU64` or `Mutex<u64>`. Two separate locks mean two creates can each read the counter before either writes, producing duplicate ids. One lock covering both fields makes read-and-insert a single atomic step.

</details>

<details>
<summary>How do I apply the auth middleware to write routes only?</summary>

Put the write routes in a separate `Router`, add `.route_layer(middleware::from_fn_with_state(...))` to it, then `.merge()` it with the public router. The logging layer goes on the merged router last, so it covers everything.

</details>

<details>
<summary>How do I check for a duplicate title?</summary>

Inside the `create_post` handler, while holding the store lock, call `.values().any(|b| b.title == payload.title)` before inserting. Return `ApiError::Conflict(...)` if it matches.

</details>

<details>
<summary>The PATCH body needs optional fields — how?</summary>

```rust
#[derive(Deserialize)]
struct UpdateBook {
    title:     Option<String>,
    author:    Option<String>,
    genre:     Option<String>,
    available: Option<bool>,
}
```

`None` means the client did not send that field. Apply only the `Some` variants to the stored book.

</details>

<details>
<summary>How do I filter by query param?</summary>

Add a `FilterParams` struct that derives `Deserialize` with `Option` fields:

```rust
#[derive(Deserialize)]
struct FilterParams {
    genre:     Option<String>,
    available: Option<bool>,
}
```

Extract it with `Query<FilterParams>` on the `list_books` handler, then filter the vec before returning.

</details>

---

## What You Are Being Assessed On

| Criterion | How it is checked |
|---|---|
| All routes return correct status codes | Acceptance script |
| Every error is JSON with `kind` | Acceptance script |
| 500s never leak internal details | Manual — compare console vs response body |
| Duplicate title returns 409 | Acceptance script |
| PATCH leaves unsupplied fields untouched | Acceptance script — checks `author` after patching only `title` |
| Auth applied to writes only, reads stay public | Acceptance script |
| Request logger prints to console | Visual — watch the terminal while the script runs |
| Atomic request counter (not a Mutex) | Code review |
| `id` and `added_at` not overwritten by PUT/PATCH | Code review |
| Genre and availability filters work | Acceptance script |

---

## Submission

Push your code to a branch named `week4-assignment` and open a pull request against `main`.
Your PR description must answer:

1. Where exactly does `?` perform a type conversion in your `create_book` handler? Which trait makes it work?
2. Why is the id counter inside the store mutex rather than a separate `AtomicU64`?
3. What would happen if you moved the `.layer(log_requests)` call above the `.merge(write_routes)` call?
