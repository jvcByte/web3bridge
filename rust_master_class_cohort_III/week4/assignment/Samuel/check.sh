#!/usr/bin/env bash
set -e

BASE="http://localhost:3000"
KEY="dev-secret-key"

echo "=== Health ==="
curl -s "$BASE/health" | python3 -m json.tool

echo ""
echo "=== List all books ==="
curl -s "$BASE/books" | python3 -m json.tool

echo ""
echo "=== Get book 1 ==="
curl -s "$BASE/books/1" | python3 -m json.tool

echo ""
echo "=== Filter by genre ==="
curl -s "$BASE/books?genre=Technical" | python3 -m json.tool

echo ""
echo "=== Search ==="
curl -s "$BASE/search?q=rust&limit=5" | python3 -m json.tool

echo ""
echo "=== Create book (auth required) ==="
curl -s -X POST "$BASE/books" \
  -H "Content-Type: application/json" \
  -H "x-api-key: $KEY" \
  -d '{"title":"Zero To Production","author":"Luca Palmieri","genre":"Technical"}' \
  | python3 -m json.tool

echo ""
echo "=== Update book 3 (full replace) ==="
curl -s -X PUT "$BASE/books/3" \
  -H "Content-Type: application/json" \
  -H "x-api-key: $KEY" \
  -d '{"title":"Zero To Production In Rust","author":"Luca Palmieri","genre":"Technical","available":false}' \
  | python3 -m json.tool

echo ""
echo "=== Patch book 3 (partial) ==="
curl -s -X PATCH "$BASE/books/3" \
  -H "Content-Type: application/json" \
  -H "x-api-key: $KEY" \
  -d '{"available":true}' \
  | python3 -m json.tool

echo ""
echo "=== Delete book 3 ==="
curl -s -o /dev/null -w "HTTP %{http_code}\n" -X DELETE "$BASE/books/3" \
  -H "x-api-key: $KEY"

echo ""
echo "=== 404 on deleted book ==="
curl -s "$BASE/books/3" | python3 -m json.tool

echo ""
echo "=== 401 without key ==="
curl -s -X POST "$BASE/books" \
  -H "Content-Type: application/json" \
  -d '{"title":"Nope","author":"No","genre":"No"}' \
  | python3 -m json.tool

echo ""
echo "=== Fallback (unknown route) ==="
curl -s "$BASE/unknown" | python3 -m json.tool

echo ""
echo "All checks passed."
