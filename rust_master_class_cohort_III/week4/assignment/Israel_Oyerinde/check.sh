#!/usr/bin/env bash
set -euo pipefail

H=localhost:3000
K=dev-secret-key

check() {
  [ "$1" = "$2" ] && echo "  ok   $3" || {
    echo "  FAIL $3: expected '$1' got '$2'"
    exit 1
  }
}

code()  { curl -s -o /dev/null -w '%{http_code}' "$@"; }
body()  { curl -s "$@"; }
field() { body "$@" | grep -o "\"$1\":\"[^\"]*\"" | head -1 | cut -d'"' -f4; }

echo "-- public reads --------------------------------------"
check 200 "$(code "$H/books")"               "GET /books"
check 200 "$(code "$H/books/1")"             "GET /books/1"
check 200 "$(code "$H/health")"              "GET /health"
check 404 "$(code "$H/books/999")"           "GET missing book -> 404"
check 404 "$(code "$H/no-such-route")"       "unknown route -> 404"

echo "-- error shape ---------------------------------------"
KIND=$(body "$H/books/999" | grep -o '"kind":"[^"]*"' | cut -d'"' -f4)
check "not_found" "$KIND"                     "404 carries kind: not_found"

echo "-- auth ----------------------------------------------"
check 401 "$(code -X POST "$H/books" \
  -H 'content-type: application/json' \
  -d '{"title":"x","author":"y","genre":"z"}')" \
  "POST without key -> 401"

check 401 "$(code -X POST "$H/books" \
  -H 'content-type: application/json' \
  -H 'x-api-key: wrong' \
  -d '{"title":"x","author":"y","genre":"z"}')" \
  "POST wrong key -> 401"

echo "-- validation ----------------------------------------"
check 400 "$(code -X POST "$H/books" \
  -H 'content-type: application/json' \
  -H "x-api-key: $K" \
  -d '{"title":"","author":"y","genre":"z"}')" \
  "empty title -> 400"

check 400 "$(code -X POST "$H/books" \
  -H 'content-type: application/json' \
  -H "x-api-key: $K" \
  -d '{"title":"ok","author":"","genre":"z"}')" \
  "empty author -> 400"

echo "-- lifecycle -----------------------------------------"
check 201 "$(code -X POST "$H/books" \
  -H 'content-type: application/json' \
  -H "x-api-key: $K" \
  -d '{"title":"Clean Code","author":"Robert Martin","genre":"Technical"}')" \
  "POST -> 201"

NEW_ID=$(body "$H/books" | grep -o '"id":[0-9]*' | tail -1 | cut -d: -f2)
check 200 "$(code "$H/books/$NEW_ID")"        "GET newly created book"

check 200 "$(code -X PATCH "$H/books/$NEW_ID" \
  -H 'content-type: application/json' \
  -H "x-api-key: $K" \
  -d '{"title":"Clean Code 2nd Ed"}')" \
  "PATCH title -> 200"

AUTHOR=$(field "author" "$H/books/$NEW_ID")
check "Robert Martin" "$AUTHOR"               "PATCH left author untouched"

check 200 "$(code -X PATCH "$H/books/$NEW_ID" \
  -H 'content-type: application/json' \
  -H "x-api-key: $K" \
  -d '{"available":false}')" \
  "PATCH available:false -> 200"

check 200 "$(code -X PUT "$H/books/$NEW_ID" \
  -H 'content-type: application/json' \
  -H "x-api-key: $K" \
  -d '{"title":"Clean Code 2nd Ed","author":"R. C. Martin","genre":"Technical","available":true}')" \
  "PUT -> 200"

check 204 "$(code -X DELETE "$H/books/$NEW_ID" -H "x-api-key: $K")" \
  "DELETE -> 204"
check 404 "$(code "$H/books/$NEW_ID")"        "deleted book -> 404"

echo "-- duplicate title -----------------------------------"
check 409 "$(code -X POST "$H/books" \
  -H 'content-type: application/json' \
  -H "x-api-key: $K" \
  -d '{"title":"The Rust Programming Language","author":"anyone","genre":"Technical"}')" \
  "duplicate title -> 409"

echo "-- filters -------------------------------------------"
check 200 "$(code "$H/books?genre=Technical")" "?genre= filter"
check 200 "$(code "$H/books?available=false")" "?available= filter"
check 200 "$(code "$H/search?q=rust")"         "?q= search"
check 200 "$(code "$H/search?q=rust&limit=1")" "?q= with ?limit="

echo "-- internal errors -----------------------------------"
echo "  verify manually: internal errors log their cause but return a generic message"

echo
echo "PASS"
