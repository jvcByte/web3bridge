# Week 4 · Day 5 — Simulation & Wrap-up

> Phase Two: Applied Rust & Systems Engineering
> Master curriculum: [`Phase_Two_Daily_Curriculum_Weeks_4-6.md`](../../../Phase_Two_Daily_Curriculum_Weeks_4-6.md)

---

## From the master curriculum

**Day 5 — Simulation & Wrap-up**
- Format note: this is an exam day, not a teaching day — no new concepts introduced.
- Scenario: *"I am your client."* Finish/harden the Blog API from scratch conditions: CRUD, validation, error handling, middleware, shared state — with minimal reference material.
- Deliverable due: working Blog Backend API. Code review + retro to close the week.

---

## ⚠️ Instructor note

**This folder is the reference solution. Do not give it to students before the simulation.**

Today is an exam. Students build from scratch conditions with minimal reference material —
official docs only, no tutorials, no copying Day 4 wholesale. This code is what you compare
against afterwards, and what you hand out at the end of the day.

The master curriculum is explicit that Day 5 is not a teaching day. Resist the urge to teach
through it.

---

## Session shape (11:00 AM – 2:00 PM)

| Time | Block |
|---|---|
| 11:00–11:15 | Brief as the client: state the requirements, take questions, do not design it for them |
| 11:15–1:15 | Students build. Circulate, pair-debug, answer with "what do the docs say?" |
| 1:15–1:40 | Acceptance run — the curl script below, against each student's server |
| 1:40–2:00 | Code review + Week 4 retro |

---

## The client brief (read this out at 11:00)

> I run a small publishing site. I need a JSON API for blog posts.
>
> Anyone should be able to read posts — a list, and one at a time by id. Only my
> editorial tool should be able to create, change, or remove them; it will send a shared secret
> in a header. When something goes wrong I need to know *what* went wrong, in JSON, with the
> right status code — my frontend branches on it. Don't ever send me a stack trace or a
> database error, my logs are public.
>
> Titles can't be empty, and I don't want two posts with the same title. When my tool sends a
> partial update, only change the fields it sent. I want to see every request in the server log
> with how long it took.
>
> It has to survive my import script, which fires fifty posts at once.

That is the whole spec. Do not translate it into endpoints for them — reading a vague
requirement and producing a route table is the skill under test.

### Acceptance criteria (share at 11:15, after questions)

- [ ] `GET /posts` and `GET /posts/{id}` are public
- [ ] `POST` / `PUT` / `PATCH` / `DELETE` require `X-API-KEY`, else 401
- [ ] Every error is JSON with a stable machine-readable `kind`
- [ ] 404 for missing posts *and* unknown routes; 400 on validation; 409 on duplicate title
- [ ] 500 responses never leak internal detail
- [ ] `PATCH` leaves unsupplied fields untouched
- [ ] Server logs method, path, status, duration
- [ ] 50 concurrent creates produce 50 distinct posts

---

## What's in this folder

The curriculum's target structure, built out:

```
day5/
├── Cargo.toml
├── src/
│   ├── lib.rs              wires the modules together
│   ├── main.rs             binds a port and serves — deliberately thin
│   ├── errors/mod.rs       ApiError + the single IntoResponse impl
│   ├── models/mod.rs       Post, CreatePost, UpdatePost + validation (6 unit tests)
│   ├── state/mod.rs        AppState, Store, the one mutex
│   ├── middleware/mod.rs   request logging, X-API-KEY auth (5 unit tests)
│   ├── handlers/
│   │   ├── mod.rs
│   │   ├── health.rs       /, /health, /about
│   │   └── posts.rs        CRUD
│   └── routes/mod.rs       the routing table, and nothing else
└── tests/
    └── api.rs              25 integration tests — the acceptance criteria, executable
```

### Why `lib.rs` *and* `main.rs`

A binary crate cannot be imported by anything, including its own `tests/` directory. Moving the
app into a library and leaving `main.rs` as a shell lets `tests/api.rs` do
`use day5_blog_api::app` and build the real router. Students routinely write everything in
`main.rs` and then find they cannot integration-test it — this is the fix, and it is worth two
minutes in the retro.

### The tests never bind a port

`tests/api.rs` uses `tower::ServiceExt::oneshot` to drive the `Router` value directly. No
`TcpListener`, no port, no `sleep` waiting for a server to come up. Every test constructs its
own `AppState`, so they are isolated and run in parallel:

```
25 integration tests + 11 unit tests, finished in 0.01s
```

Show them this. The instinct is to spawn a server and `curl` it from the test, which is slow,
flaky, and serialises on a fixed port.

---

## Running it

```bash
cargo run                       # http://127.0.0.1:3000
API_KEY=hunter2 PORT=8080 cargo run
cargo test                      # 36 tests, no port needed
```

### Acceptance run (1:15 block)

Paste this against each student's server. It exits non-zero on the first failure.

```bash
#!/usr/bin/env bash
set -euo pipefail
H=localhost:3000
K=dev-secret-key

check() { # check <expected> <actual> <label>
  [ "$1" = "$2" ] && echo "  ok   $3" || { echo "  FAIL $3: expected $1 got $2"; exit 1; }
}
code() { curl -s -o /dev/null -w '%{http_code}' "$@"; }

echo "public reads"
check 200 "$(code $H/posts)"                                    "GET /posts"
check 200 "$(code $H/posts/1)"                                  "GET /posts/1"
check 404 "$(code $H/posts/999)"                                "GET missing -> 404"
check 404 "$(code $H/no-such-route)"                            "unknown route -> 404"

echo "auth"
check 401 "$(code -X POST $H/posts -H 'content-type: application/json' -d '{"title":"a","body":"b"}')" \
                                                                "POST without key -> 401"
check 401 "$(code -X POST $H/posts -H 'content-type: application/json' -H 'x-api-key: wrong' -d '{"title":"a","body":"b"}')" \
                                                                "POST wrong key -> 401"

echo "validation"
check 400 "$(code -X POST $H/posts -H 'content-type: application/json' -H "x-api-key: $K" -d '{"title":"","body":"b"}')" \
                                                                "empty title -> 400"
check 409 "$(code -X POST $H/posts -H 'content-type: application/json' -H "x-api-key: $K" -d '{"title":"Futures are lazy","body":"dup"}')" \
                                                                "duplicate title -> 409"

echo "lifecycle"
check 201 "$(code -X POST $H/posts -H 'content-type: application/json' -H "x-api-key: $K" -d '{"title":"Acceptance","body":"created"}')" \
                                                                "POST -> 201"
check 200 "$(code -X PATCH $H/posts/3 -H 'content-type: application/json' -H "x-api-key: $K" -d '{"title":"Accepted"}')" \
                                                                "PATCH -> 200"
BODY=$(curl -s $H/posts/3 | grep -o '"body":"[^"]*"')
check '"body":"created"' "$BODY"                                "PATCH left body untouched"
check 204 "$(code -X DELETE $H/posts/3 -H "x-api-key: $K")"     "DELETE -> 204"
check 404 "$(code $H/posts/3)"                                  "deleted -> 404"

echo "error shape"
KIND=$(curl -s $H/posts/999 | grep -o '"kind":"[^"]*"')
check '"kind":"not_found"' "$KIND"                              "errors carry a kind"

echo "concurrency — the import script"
BEFORE=$(curl -s "$H/posts?limit=200" | grep -o '"id"' | wc -l)
seq 1 50 | xargs -P 50 -I{} curl -s -o /dev/null -X POST $H/posts \
  -H 'content-type: application/json' -H "x-api-key: $K" \
  -d '{"title":"import {}","body":"bulk"}'
AFTER=$(curl -s "$H/posts?limit=200" | grep -o '"id"' | wc -l)
check $((BEFORE + 50)) "$AFTER"                                 "50 concurrent creates -> 50 posts"

echo
echo "PASS"
```

The concurrency check is the one that separates a working submission from one that merely
responds. A separate `Mutex<u64>` for the id counter passes every other check and fails this
one intermittently — which is exactly what makes it a good exam question.

---

## What to look for during review (1:40)

Common failure modes, roughly in order of how often they appear:

- **Id counter outside the store mutex.** Two locks, so two creates read the same id. Passes
  casual testing, fails under `xargs -P 50`.
- **`.unwrap()` on the lock everywhere.** Works until one handler panics and poisons the mutex,
  after which every subsequent request panics too.
- **500s leaking internals.** `ApiError::Internal(e.to_string())` rendered straight into the
  response body.
- **`layer` instead of `route_layer` on the auth router**, so unknown paths return 401 and
  reveal that the route doesn't exist.
- **PATCH implemented with the same struct as PUT**, so omitted fields get clobbered with
  defaults or empty strings.
- **`Json` extractor not last**, producing a trait-bound error wall the student cannot read.
- **`HashMap` iteration returned unsorted**, so `GET /posts` shuffles between requests and they
  chase a phantom bug.
- **No fallback handler**, so unknown routes return an empty-bodied 404 that breaks the client's
  error parsing.

---

## Week 4 retro (1:40–2:00)

Run this as an interview simulation — ask, let them answer aloud, do not accept the first
sentence.

- Why Axum instead of Actix?
- What actually happens when `.await` is called?
- Why is `Arc<Mutex<T>>` needed? Why not `Rc<RefCell<T>>`?
- Where exactly does `?` convert one error type into another?
- What is the difference between `layer` and `route_layer`?
- Which crate surprised you this week? What did you read in it?

Then set up Week 5: *this API speaks HTTP because `hyper` frames the bytes for you. Next week
you get a raw TCP socket and no framing at all — you'll have to decide where one message ends
and the next begins.*

---

## Homework

Over the weekend, pick two:

- Swap the `HashMap` for `sqlx` + SQLite. Note how much of `handlers/` has to change (less than
  you'd expect — that is what the module split bought you).
- Replace the epoch-second `created_at` with `chrono::DateTime<Utc>` and make it serialise as
  RFC 3339.
- Add `tower_http::trace::TraceLayer` next to the hand-written logger and compare.
- Add pagination metadata (`total`, `hasMore`) to the list response.
- Write a test that proves a poisoned mutex returns 500 rather than panicking the process.
