#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: bash script/verify_pr4_2_backend_auth.sh [--start-db]

Checks the PR-4.2 backend authentication implementation.

Default mode:
  - verifies expected backend auth files
  - validates docker-compose.dev.yml when Docker Compose is available
  - runs backend cargo fmt/check/test
  - runs PostgreSQL integration tests only when OPENKOTO_TEST_DATABASE_URL is set

Options:
  --start-db   Start the PostgreSQL development container and use it as OPENKOTO_TEST_DATABASE_URL.
USAGE
}

START_DB=0
for arg in "$@"; do
  case "$arg" in
    --start-db)
      START_DB=1
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg" >&2
      usage >&2
      exit 2
      ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

DEFAULT_DATABASE_URL="postgres://openkoto:openkoto_dev_password@127.0.0.1:5433/openkoto_dev"

info() {
  printf '[pr4.2] %s\n' "$1"
}

warn() {
  printf '[pr4.2][warn] %s\n' "$1" >&2
}

fail() {
  printf '[pr4.2][error] %s\n' "$1" >&2
  exit 1
}

compose() {
  if docker compose version >/dev/null 2>&1; then
    docker compose "$@"
  elif command -v docker-compose >/dev/null 2>&1; then
    docker-compose "$@"
  else
    return 127
  fi
}

require_file() {
  local file="$1"
  if [ ! -f "$file" ]; then
    fail "Missing required file: $file"
  fi
  info "Found $file"
}

require_file "docker-compose.dev.yml"
require_file "openkoto-backend/Cargo.toml"
require_file "openkoto-backend/src/auth.rs"
require_file "openkoto-backend/migrations/20260707000200_auth.sql"
require_file "openkoto-backend/README.md"
require_file "openkoto-backend/VERIFICATION.md"
require_file "openkoto-backend/migrations/README.md"

if ! grep -q "CREATE TABLE users" openkoto-backend/migrations/20260707000200_auth.sql; then
  fail "Auth migration does not create users table"
fi
if ! grep -q "CREATE TABLE sessions" openkoto-backend/migrations/20260707000200_auth.sql; then
  fail "Auth migration does not create sessions table"
fi
if ! grep -q "token_hash" openkoto-backend/migrations/20260707000200_auth.sql; then
  fail "Auth migration should store token_hash, not raw token"
fi

if compose -f docker-compose.dev.yml config >/dev/null; then
  info "docker-compose.dev.yml is valid"
else
  if command -v docker >/dev/null 2>&1 || command -v docker-compose >/dev/null 2>&1; then
    fail "docker-compose.dev.yml validation failed"
  fi
  warn "Docker Compose is not available; skipped compose validation"
fi

if [ "$START_DB" -eq 1 ]; then
  if ! command -v docker >/dev/null 2>&1; then
    fail "Docker is required for --start-db"
  fi
  if ! docker info >/dev/null 2>&1; then
    fail "Docker daemon is not running or is not reachable"
  fi

  info "Starting PostgreSQL development container"
  compose -f docker-compose.dev.yml up -d openkoto-postgres

  container_id="$(compose -f docker-compose.dev.yml ps -q openkoto-postgres)"
  if [ -z "$container_id" ]; then
    fail "Could not locate openkoto-postgres container"
  fi

  info "Waiting for PostgreSQL health"
  for _ in $(seq 1 40); do
    status="$(docker inspect -f '{{.State.Health.Status}}' "$container_id" 2>/dev/null || true)"
    if [ "$status" = "healthy" ]; then
      info "PostgreSQL is healthy"
      break
    fi
    sleep 1
  done

  status="$(docker inspect -f '{{.State.Health.Status}}' "$container_id" 2>/dev/null || true)"
  if [ "$status" != "healthy" ]; then
    compose -f docker-compose.dev.yml logs --tail=80 openkoto-postgres >&2 || true
    fail "PostgreSQL did not become healthy; status=$status"
  fi

  export OPENKOTO_TEST_DATABASE_URL="${OPENKOTO_TEST_DATABASE_URL:-$DEFAULT_DATABASE_URL}"
  info "Using OPENKOTO_TEST_DATABASE_URL=$OPENKOTO_TEST_DATABASE_URL"
elif [ -z "${OPENKOTO_TEST_DATABASE_URL:-}" ]; then
  warn "OPENKOTO_TEST_DATABASE_URL is not set; PostgreSQL integration tests will be skipped"
else
  info "Using existing OPENKOTO_TEST_DATABASE_URL"
fi

if ! command -v cargo >/dev/null 2>&1; then
  fail "Cargo is required"
fi

info "Running backend cargo fmt --check"
cargo fmt --manifest-path openkoto-backend/Cargo.toml --check

info "Running backend cargo check"
cargo check --manifest-path openkoto-backend/Cargo.toml

info "Running backend cargo test"
cargo test --manifest-path openkoto-backend/Cargo.toml

info "PR-4.2 backend auth checks completed"
