# Book Library API Documentation

This document describes every HTTP request supported by the Book Library API. It includes REST request details for tools such as Postman and copy-ready `curl` examples.

## Base URL and Variables

The examples use the local development server:

```text
http://localhost:3000
```

For Postman or another REST client, these variables are convenient:

| Variable | Example value | Description |
|---|---|---|
| `baseUrl` | `http://localhost:3000` | Address where the API is running. |
| `apiKey` | `<YOUR_API_KEY>` | API key configured on the server. |

Start the server with an API key of your choice:

```bash
API_KEY="<YOUR_API_KEY>" cargo run
```

## Authentication and Headers

Read requests are public. Create, replace, update, and delete requests require an API key.

| Header | Value | When to send it |
|---|---|---|
| `Accept` | `application/json` | Optional on all requests. It tells the server that the client expects JSON. |
| `Content-Type` | `application/json` | Required for `POST`, `PUT`, and `PATCH` requests with a JSON body. |
| `X-API-KEY` | `<YOUR_API_KEY>` | Required for `POST`, `PUT`, `PATCH`, and `DELETE`. |

The server reads the expected key from its `API_KEY` environment variable. A missing or incorrect key returns `401 Unauthorized`. The key is compared using a constant-time comparison, and authentication is applied only to write routes.

## Endpoint Summary

| Method | Path | Authentication | Purpose |
|---|---|---|---|
| `GET` | `/books` | Public | List and filter books. |
| `GET` | `/books/{id}` | Public | Get one book by ID. |
| `GET` | `/search` | Public | Search titles and authors. |
| `GET` | `/health` | Public | Check API health and book count. |
| `POST` | `/books` | API key | Create a book. |
| `PUT` | `/books/{id}` | API key | Fully replace a book. |
| `PATCH` | `/books/{id}` | API key | Partially update a book. |
| `DELETE` | `/books/{id}` | API key | Delete a book. |

## Book Object

Successful book responses use this shape:

```json
{
  "id": 1,
  "title": "The Rust Programming Language",
  "author": "Steve Klabnik",
  "genre": "Technical",
  "available": true,
  "addedAt": "2026-08-16T14:40:20Z"
}
```

| Field | JSON type | Description |
|---|---|---|
| `id` | number | Server-assigned unsigned integer. Clients cannot set or change it. |
| `title` | string | Required, non-empty, and no more than 150 characters. |
| `author` | string | Required and non-empty. |
| `genre` | string | Required and non-empty. |
| `available` | boolean | Whether the book is available. Defaults to `true` when omitted during creation. |
| `addedAt` | string | Server-assigned RFC 3339 timestamp. Clients cannot set or change it. |

## Error Format

Errors are returned as JSON:

```json
{
  "error": {
    "kind": "not_found",
    "message": "book 42 not found"
  }
}
```

| HTTP status | `kind` | Meaning |
|---|---|---|
| `400 Bad Request` | `validation_failed` | A field, JSON body, path parameter, or query parameter is invalid. |
| `401 Unauthorized` | `unauthorized` | The API key is missing or incorrect. |
| `404 Not Found` | `not_found` | A book or route does not exist. |
| `405 Method Not Allowed` | `method_not_allowed` | The path exists but does not support the requested method. |
| `409 Conflict` | `conflict` | Another book already uses the submitted title. |
| `500 Internal Server Error` | `internal_error` | An unexpected server error occurred. Internal details are not exposed. |

Example validation error:

```json
{
  "error": {
    "kind": "validation_failed",
    "message": "title must contain at least one non-whitespace character"
  }
}
```

Example authentication error:

```json
{
  "error": {
    "kind": "unauthorized",
    "message": "missing or invalid API key"
  }
}
```

Example internal error:

```json
{
  "error": {
    "kind": "internal_error",
    "message": "an internal server error occurred"
  }
}
```

## 1. List Books

Returns all books sorted by `id`. Optional query parameters can filter the result.

| Property | Value |
|---|---|
| Method | `GET` |
| URL | `{{baseUrl}}/books` |
| Authentication | Not required |
| Request body | None |
| Success status | `200 OK` |

### Query Parameters

| Parameter | Type | Required | Description |
|---|---|---|---|
| `genre` | string | No | Returns books whose genre matches the value, ignoring ASCII letter case. |
| `available` | boolean | No | Accepts `true` or `false` and returns books with that availability. |

### REST Request

```http
GET /books?genre=Technical&available=true HTTP/1.1
Host: localhost:3000
Accept: application/json
```

### curl

```bash
curl --request GET \
  --url "http://localhost:3000/books?genre=Technical&available=true" \
  --header "Accept: application/json"
```

### Success Response

Status: `200 OK`

```json
[
  {
    "id": 1,
    "title": "The Rust Programming Language",
    "author": "Steve Klabnik",
    "genre": "Technical",
    "available": true,
    "addedAt": "2026-08-16T14:40:20Z"
  }
]
```

An empty match returns `200 OK` with `[]`.

### Possible Errors

| Status | Cause |
|---|---|
| `400` | `available` is not `true` or `false`, or another query value cannot be parsed. |
| `500` | The server cannot access the book store. |

## 2. Get One Book

Returns one book by its numeric ID.

| Property | Value |
|---|---|
| Method | `GET` |
| URL | `{{baseUrl}}/books/{id}` |
| Authentication | Not required |
| Request body | None |
| Success status | `200 OK` |

### Path Parameters

| Parameter | Type | Required | Description |
|---|---|---|---|
| `id` | unsigned integer | Yes | ID of the book to retrieve. |

### REST Request

```http
GET /books/1 HTTP/1.1
Host: localhost:3000
Accept: application/json
```

### curl

```bash
curl --request GET \
  --url "http://localhost:3000/books/1" \
  --header "Accept: application/json"
```

### Success Response

Status: `200 OK`

```json
{
  "id": 1,
  "title": "The Rust Programming Language",
  "author": "Steve Klabnik",
  "genre": "Technical",
  "available": true,
  "addedAt": "2026-08-16T14:40:20Z"
}
```

### Possible Errors

| Status | Cause |
|---|---|
| `400` | `id` is not a valid unsigned integer. |
| `404` | No book exists with the supplied ID. |
| `500` | The server cannot access the book store. |

Example `404` response:

```json
{
  "error": {
    "kind": "not_found",
    "message": "book 999 not found"
  }
}
```

## 3. Search Books

Searches book titles and authors without case sensitivity. Results are sorted by `id`.

| Property | Value |
|---|---|
| Method | `GET` |
| URL | `{{baseUrl}}/search` |
| Authentication | Not required |
| Request body | None |
| Success status | `200 OK` |

### Query Parameters

| Parameter | Type | Required | Default | Description |
|---|---|---|---|---|
| `q` | string | No | Empty string | Text to find in the title or author. An omitted or empty value matches all books. |
| `limit` | non-negative integer | No | `10` | Maximum number of results to return. |

### REST Request

```http
GET /search?q=rust&limit=1 HTTP/1.1
Host: localhost:3000
Accept: application/json
```

### curl

```bash
curl --get "http://localhost:3000/search" \
  --header "Accept: application/json" \
  --data-urlencode "q=rust" \
  --data-urlencode "limit=1"
```

### Success Response

Status: `200 OK`

```json
[
  {
    "id": 1,
    "title": "The Rust Programming Language",
    "author": "Steve Klabnik",
    "genre": "Technical",
    "available": true,
    "addedAt": "2026-08-16T14:40:20Z"
  }
]
```

### Possible Errors

| Status | Cause |
|---|---|
| `400` | `limit` is not a valid non-negative integer. |
| `500` | The server cannot access the book store. |

## 4. Health Check

Confirms that the API is running and reports the current number of books.

| Property | Value |
|---|---|
| Method | `GET` |
| URL | `{{baseUrl}}/health` |
| Authentication | Not required |
| Request body | None |
| Success status | `200 OK` |

### REST Request

```http
GET /health HTTP/1.1
Host: localhost:3000
Accept: application/json
```

### curl

```bash
curl --request GET \
  --url "http://localhost:3000/health" \
  --header "Accept: application/json"
```

### Success Response

Status: `200 OK`

```json
{
  "status": "ok",
  "books": 2
}
```

### Possible Errors

| Status | Cause |
|---|---|
| `500` | The server cannot access the book store. |

## 5. Create a Book

Creates a book and assigns its `id` and `addedAt` values on the server.

| Property | Value |
|---|---|
| Method | `POST` |
| URL | `{{baseUrl}}/books` |
| Authentication | `X-API-KEY` required |
| Content type | `application/json` |
| Success status | `201 Created` |

### Request Headers

| Header | Value | Required |
|---|---|---|
| `Content-Type` | `application/json` | Yes |
| `Accept` | `application/json` | No |
| `X-API-KEY` | `<YOUR_API_KEY>` | Yes |

### Request Body

| Field | Type | Required | Description |
|---|---|---|---|
| `title` | string | Yes | Non-empty and at most 150 characters. Must be unique. |
| `author` | string | Yes | Non-empty author name. |
| `genre` | string | Yes | Non-empty genre. |
| `available` | boolean | No | Defaults to `true` when omitted. |

Do not send `id` or `addedAt`; both fields are owned by the server.

```json
{
  "title": "Clean Code",
  "author": "Robert C. Martin",
  "genre": "Technical",
  "available": true
}
```

### REST Request

```http
POST /books HTTP/1.1
Host: localhost:3000
Accept: application/json
Content-Type: application/json
X-API-KEY: <YOUR_API_KEY>

{
  "title": "Clean Code",
  "author": "Robert C. Martin",
  "genre": "Technical",
  "available": true
}
```

### curl

```bash
curl --request POST \
  --url "http://localhost:3000/books" \
  --header "Accept: application/json" \
  --header "Content-Type: application/json" \
  --header "X-API-KEY: <YOUR_API_KEY>" \
  --data '{
    "title": "Clean Code",
    "author": "Robert C. Martin",
    "genre": "Technical",
    "available": true
  }'
```

### Success Response

Status: `201 Created`

```json
{
  "id": 3,
  "title": "Clean Code",
  "author": "Robert C. Martin",
  "genre": "Technical",
  "available": true,
  "addedAt": "2026-08-16T15:00:00Z"
}
```

### Possible Errors

| Status | Cause |
|---|---|
| `400` | JSON is malformed, a required field is absent, a text field is empty, the title is too long, or a server-owned field was supplied. |
| `401` | `X-API-KEY` is missing or incorrect. |
| `409` | A book with the same title already exists. |
| `500` | The server cannot access the book store. |

Example `409` response:

```json
{
  "error": {
    "kind": "conflict",
    "message": "a book titled \"Clean Code\" already exists"
  }
}
```

## 6. Replace a Book

Fully replaces the editable fields of an existing book. The server preserves `id` and `addedAt`.

| Property | Value |
|---|---|
| Method | `PUT` |
| URL | `{{baseUrl}}/books/{id}` |
| Authentication | `X-API-KEY` required |
| Content type | `application/json` |
| Success status | `200 OK` |

### Path Parameters

| Parameter | Type | Required | Description |
|---|---|---|---|
| `id` | unsigned integer | Yes | ID of the book to replace. |

### Request Headers

| Header | Value | Required |
|---|---|---|
| `Content-Type` | `application/json` | Yes |
| `Accept` | `application/json` | No |
| `X-API-KEY` | `<YOUR_API_KEY>` | Yes |

### Request Body

All four fields are required for a full replacement.

| Field | Type | Required | Description |
|---|---|---|---|
| `title` | string | Yes | Non-empty, at most 150 characters, and unique. |
| `author` | string | Yes | Non-empty author name. |
| `genre` | string | Yes | Non-empty genre. |
| `available` | boolean | Yes | New availability value. |

Do not send `id` or `addedAt`; both fields are preserved by the server.

```json
{
  "title": "Clean Code, Second Edition",
  "author": "Robert C. Martin",
  "genre": "Technical",
  "available": false
}
```

### REST Request

```http
PUT /books/3 HTTP/1.1
Host: localhost:3000
Accept: application/json
Content-Type: application/json
X-API-KEY: <YOUR_API_KEY>

{
  "title": "Clean Code, Second Edition",
  "author": "Robert C. Martin",
  "genre": "Technical",
  "available": false
}
```

### curl

```bash
curl --request PUT \
  --url "http://localhost:3000/books/3" \
  --header "Accept: application/json" \
  --header "Content-Type: application/json" \
  --header "X-API-KEY: <YOUR_API_KEY>" \
  --data '{
    "title": "Clean Code, Second Edition",
    "author": "Robert C. Martin",
    "genre": "Technical",
    "available": false
  }'
```

### Success Response

Status: `200 OK`

```json
{
  "id": 3,
  "title": "Clean Code, Second Edition",
  "author": "Robert C. Martin",
  "genre": "Technical",
  "available": false,
  "addedAt": "2026-08-16T15:00:00Z"
}
```

### Possible Errors

| Status | Cause |
|---|---|
| `400` | `id` is invalid, JSON is malformed, a required field is missing, validation fails, or a server-owned field was supplied. |
| `401` | `X-API-KEY` is missing or incorrect. |
| `404` | No book exists with the supplied ID. |
| `409` | Another book already uses the submitted title. |
| `500` | The server cannot access the book store. |

## 7. Partially Update a Book

Updates only the fields included in the request body. Fields that are not supplied remain unchanged.

| Property | Value |
|---|---|
| Method | `PATCH` |
| URL | `{{baseUrl}}/books/{id}` |
| Authentication | `X-API-KEY` required |
| Content type | `application/json` |
| Success status | `200 OK` |

### Path Parameters

| Parameter | Type | Required | Description |
|---|---|---|---|
| `id` | unsigned integer | Yes | ID of the book to update. |

### Request Headers

| Header | Value | Required |
|---|---|---|
| `Content-Type` | `application/json` | Yes |
| `Accept` | `application/json` | No |
| `X-API-KEY` | `<YOUR_API_KEY>` | Yes |

### Request Body

Every field is optional, but only the listed fields are accepted.

| Field | Type | Required | Description |
|---|---|---|---|
| `title` | string | No | If supplied, must be non-empty, at most 150 characters, and unique. |
| `author` | string | No | If supplied, must be non-empty. |
| `genre` | string | No | If supplied, must be non-empty. |
| `available` | boolean | No | May be set to either `true` or `false`. |

Do not send `id` or `addedAt`; both fields are owned by the server.

```json
{
  "available": false
}
```

### REST Request

```http
PATCH /books/3 HTTP/1.1
Host: localhost:3000
Accept: application/json
Content-Type: application/json
X-API-KEY: <YOUR_API_KEY>

{
  "available": false
}
```

### curl

```bash
curl --request PATCH \
  --url "http://localhost:3000/books/3" \
  --header "Accept: application/json" \
  --header "Content-Type: application/json" \
  --header "X-API-KEY: <YOUR_API_KEY>" \
  --data '{
    "available": false
  }'
```

### Success Response

Status: `200 OK`

```json
{
  "id": 3,
  "title": "Clean Code, Second Edition",
  "author": "Robert C. Martin",
  "genre": "Technical",
  "available": false,
  "addedAt": "2026-08-16T15:00:00Z"
}
```

### Possible Errors

| Status | Cause |
|---|---|
| `400` | `id` is invalid, JSON is malformed, a supplied field fails validation, or a server-owned field was supplied. |
| `401` | `X-API-KEY` is missing or incorrect. |
| `404` | No book exists with the supplied ID. |
| `409` | Another book already uses the submitted title. |
| `500` | The server cannot access the book store. |

## 8. Delete a Book

Permanently removes a book from the in-memory store.

| Property | Value |
|---|---|
| Method | `DELETE` |
| URL | `{{baseUrl}}/books/{id}` |
| Authentication | `X-API-KEY` required |
| Request body | None |
| Success status | `204 No Content` |

### Path Parameters

| Parameter | Type | Required | Description |
|---|---|---|---|
| `id` | unsigned integer | Yes | ID of the book to delete. |

### Request Headers

| Header | Value | Required |
|---|---|---|
| `Accept` | `application/json` | No |
| `X-API-KEY` | `<YOUR_API_KEY>` | Yes |

### REST Request

```http
DELETE /books/3 HTTP/1.1
Host: localhost:3000
Accept: application/json
X-API-KEY: <YOUR_API_KEY>
```

### curl

```bash
curl --request DELETE \
  --url "http://localhost:3000/books/3" \
  --header "Accept: application/json" \
  --header "X-API-KEY: <YOUR_API_KEY>"
```

### Success Response

Status: `204 No Content`

The response has no body.

### Possible Errors

| Status | Cause |
|---|---|
| `400` | `id` is not a valid unsigned integer. |
| `401` | `X-API-KEY` is missing or incorrect. |
| `404` | No book exists with the supplied ID. |
| `500` | The server cannot access the book store. |

## Unknown Routes

A request to a route that does not exist returns `404 Not Found` with JSON:

```json
{
  "error": {
    "kind": "not_found",
    "message": "no route for /no-such-route"
  }
}
```
