# Week 4 · Day 3 — CRUD + Shared State

> Phase Two: Applied Rust & Systems Engineering
> Master curriculum: [`Phase_Two_Daily_Curriculum_Weeks_4-6.md`](../../../Phase_Two_Daily_Curriculum_Weeks_4-6.md)

---

## From the master curriculum

**Day 3 — CRUD + Shared State**
- Pre-class: read the Axum `todos` example line by line.
- Topics: full CRUD for `Post`; `Vec<Post>` (or `HashMap`) inside `Arc<Mutex<_>>`; `AppState`; what shared state does to ownership.
- Build: `POST /posts`, `GET /posts`, `GET /posts/:id`, `PUT /posts/:id`, `DELETE /posts/:id`. Create + Read are the in-class minimum bar; Update/Delete can finish as homework, checked at the top of Day 4.
- Resources: https://github.com/tokio-rs/axum/tree/main/examples/todos

---

## Session shape (11:00 AM – 2:00 PM)

| Time | Block |
|---|---|
| 11:00–11:25 | Concept — why `Arc<Mutex<T>>`, and why not `Rc<RefCell<T>>` |
| 11:25–12:10 | Live coding — `AppState`, `State` extractor, Create + Read |
| 12:10–12:25 | Break |
| 12:25–1:35 | Student implementation — Create + Read is the bar, then Update/Delete |
| 1:35–2:00 | Code review + debugging |

**In-class minimum bar: `POST /posts` and `GET /posts` persisting correctly.** Update and
Delete are legitimate homework — check them at the top of Day 4.

---

## What's in this folder

```
day3/
├── Cargo.toml
└── src/
    └── main.rs
```

Full CRUD, all of it backed by one shared store:

| Method | Path | Status on success | Status on miss |
|---|---|---|---|
| `GET` | `/posts` | 200 | — |
| `POST` | `/posts` | 201 | — |
| `GET` | `/posts/{id}` | 200 | 404 |
| `PUT` | `/posts/{id}` | 200 | 404 |
| `PATCH` | `/posts/{id}` | 200 | 404 |
| `DELETE` | `/posts/{id}` | 204 | 404 |

The store is a `HashMap<u64, Post>`, not a `Vec<Post>`. Both appear in the curriculum; the
`HashMap` is the better choice here and the reason is worth two minutes in class. With a `Vec`,
`GET /posts/{id}` is an O(n) scan and deleting shifts every later element — and if you use the
vector index as the id, deleting post 2 silently renumbers post 3. Keys that change identity
when unrelated rows are removed is a real bug class, not a style preference.

`Vec` is still the right call when you need ordering, which is why `list_posts` sorts on the way
out.

---

## Running it

```bash
cargo run
```

```bash
# create — and this time it sticks
curl -X POST localhost:3000/posts \
  -H 'content-type: application/json' \
  -d '{"title":"Shared state","body":"Arc<Mutex<T>>","author":"Ada"}'

curl localhost:3000/posts          # it is actually there now
curl localhost:3000/posts/3

# full replace
curl -X PUT localhost:3000/posts/3 \
  -H 'content-type: application/json' \
  -d '{"title":"Replaced","body":"All fields required","author":"Grace"}'

# partial update — only the fields you send
curl -X PATCH localhost:3000/posts/3 \
  -H 'content-type: application/json' \
  -d '{"title":"Just the title"}'

curl -i -X DELETE localhost:3000/posts/3   # 204
curl -i localhost:3000/posts/3             # 404
```

Prove the state is genuinely shared across concurrent connections:

```bash
# 20 posts created in parallel from 20 sockets
seq 1 20 | xargs -P 20 -I{} curl -s -o /dev/null -X POST localhost:3000/posts \
  -H 'content-type: application/json' -d '{"title":"post {}","body":"concurrent"}'

curl -s localhost:3000/posts | grep -o '"id"' | wc -l   # expect 22 (2 seeded + 20)
```

That last check is the day's real assertion: no lost updates, no duplicate ids, under genuine
parallelism. Run it a few times.

---

## Concept notes for the 11:00 block

**Why anything at all.** Yesterday's handlers could not persist because each one owned only
what the request gave it. To share, every handler needs a reference to the same value — and
axum hands each request to a task that may be on any worker thread. So the shared value must
be reachable from multiple owners (`Arc`) and safe to mutate from multiple threads (`Mutex`).
That is the entire derivation of `Arc<Mutex<T>>`. Do not present it as an incantation; derive
it from those two requirements in front of them.

**Why not `Rc<RefCell<T>>`.** This is on the interview list. `Rc`'s refcount is a plain
non-atomic integer — two threads cloning at once can interleave the read-modify-write and lose
a count, giving you a double free. So `Rc` is `!Send`, and `RefCell`'s borrow flag has the same
problem, so it is `!Sync`. `tokio::spawn` requires `Send`, so the compiler rejects them before
you can make the mistake. Fearless concurrency is the compiler refusing to let you ship the
data race, not the absence of the hazard.

**`Arc` and `Mutex` do different jobs.** Students routinely think one implies the other. `Arc`
gives *shared ownership* — N owners, freed when the last drops. `Mutex` gives *exclusive
access* — one writer at a time. You need both because you have both problems. `Arc<T>` alone
gives you shared immutable access; `Mutex<T>` alone can't be shared without an owner to hang it
off. Ask: what does `Arc<Vec<Post>>` let you do? (Read. Nothing more.)

**Hold the lock for as little as possible.** Look at how every handler in `main.rs` takes the
lock, does one thing, and drops it — often via an explicit inner scope. `.await` while holding
a `std::sync::MutexGuard` will not compile under `spawn`, because the guard is `!Send`. That
compile error is a gift; it is the compiler catching a class of deadlock at build time. Show it
deliberately: add `sleep(...).await` inside a locked scope and read the error.

**When `tokio::sync::Mutex` instead.** Only when you genuinely must hold the lock across an
`.await` — an in-flight DB transaction, say. It is slower, since contention parks the task
rather than spinning briefly. Default to `std::sync::Mutex` for plain data like this. Week 6
Day 1 revisits this with `RwLock`.

**Why `Arc<AppState>` and not `#[derive(Clone)] AppState`.** Both work. The struct holding one
already-`Arc`ed field and deriving `Clone` is the idiom in the axum examples. Wrapping the
whole state in one `Arc` means adding a second field later costs nothing and there is exactly
one refcount to reason about. This file uses `Arc<AppState>` for that reason; point out the
alternative so they recognise it in the `todos` example.

**Where the id comes from.** `next_id` lives *inside* the same mutex as the map, not beside it.
Two separate locks would let two requests read the same id before either writes — a textbook
race. One lock covering both fields makes "read the counter and insert" a single atomic step.
This is worth drawing on the board; it is the same reasoning students will need in Week 6.

---

## Talking points during code review (1:35)

- Where exactly is the lock acquired and released in `create_post`? Could two requests get the
  same id?
- What does `.lock().unwrap()` unwrap, and when is it actually `Err`? (Poisoning — another
  thread panicked while holding it.)
- Why `HashMap` over `Vec` here?
- Why does `DELETE` return 204 and not 200 with a body?
- What is the difference between `PUT` and `PATCH` in this file, and which one needs
  `Option<T>` fields?
- Try adding an `.await` inside a locked scope. Read the error. Why is that error good news?

---

## Homework (standing)

- Read the official docs for today's topic — `std::sync::Mutex`, `axum::extract::State`.
- Read one crate's source for 15–20 minutes. Today: `std::sync::Arc` — find the atomic
  refcount increment.
- Solve or review one Rustlings exercise.
- Refactor yesterday's code.
- Write at least one unit test.

Today specifically: finish `PUT` and `DELETE` if you did not get there in class — checked at the
top of Day 4. Then add `GET /posts?author=Ada` filtering, and make `list_posts` support
`?limit=` and `?offset=`.
