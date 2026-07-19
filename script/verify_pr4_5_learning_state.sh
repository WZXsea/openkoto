#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: bash script/verify_pr4_5_learning_state.sh [--start-db] [--skip-frontend]

Checks the PR-4.5 backend learning-state implementation and Desktop integration.

Default mode:
  - verifies expected backend learning-state files
  - validates docker-compose.dev.yml when Docker Compose is available
  - runs backend cargo fmt/check/test
  - runs Desktop cargo check/test
  - runs Desktop npm typecheck/test unless --skip-frontend is passed
  - runs PostgreSQL integration tests only when OPENKOTO_TEST_DATABASE_URL is set

Options:
  --start-db       Start the PostgreSQL development container and use it as OPENKOTO_TEST_DATABASE_URL.
  --skip-frontend  Skip npm typecheck and Vitest.

Environment:
  OPENKOTO_POSTGRES_PORT  Host port for the dev PostgreSQL container. Defaults to 5433.
USAGE
}

START_DB=0
SKIP_FRONTEND=0
for arg in "$@"; do
  case "$arg" in
    --start-db)
      START_DB=1
      ;;
    --skip-frontend)
      SKIP_FRONTEND=1
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

POSTGRES_PORT="${OPENKOTO_POSTGRES_PORT:-5433}"
DEFAULT_DATABASE_URL="postgres://openkoto:openkoto_dev_password@127.0.0.1:${POSTGRES_PORT}/openkoto_dev"

info() {
  printf '[pr4.5] %s\n' "$1"
}

warn() {
  printf '[pr4.5][warn] %s\n' "$1" >&2
}

fail() {
  printf '[pr4.5][error] %s\n' "$1" >&2
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

require_file "openkoto-backend/src/learning.rs"
require_file "openkoto-backend/tests/learning_state.rs"
require_file "openkoto-backend/migrations/20260707000400_learning_state.sql"
require_file "textlingo-desktop/src-tauri/src/backend_client.rs"
require_file "textlingo-desktop/src-tauri/src/commands.rs"
require_file "textlingo-desktop/src-tauri/src/agent_worker.rs"

for expected in \
  "CREATE TABLE word_packs" \
  "CREATE TABLE favorite_vocabularies" \
  "CREATE TABLE favorite_vocabulary_packs" \
  "CREATE TABLE favorite_grammars" \
  "CREATE TABLE bookmarks" \
  "CREATE TABLE agent_tasks" \
  "CREATE TABLE artifacts"; do
  if ! grep -q "$expected" openkoto-backend/migrations/20260707000400_learning_state.sql; then
    fail "Learning-state migration missing: $expected"
  fi
done

if ! grep -q "AuthenticatedUser" openkoto-backend/src/learning.rs; then
  fail "learning.rs should require authenticated users"
fi
if ! grep -q "list_favorite_vocabularies" textlingo-desktop/src-tauri/src/backend_client.rs; then
  fail "backend_client.rs should expose learning-state client methods"
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

info "Running Desktop cargo check"
cargo check --manifest-path textlingo-desktop/src-tauri/Cargo.toml

info "Running Desktop cargo test"
cargo test --manifest-path textlingo-desktop/src-tauri/Cargo.toml

if [ "$SKIP_FRONTEND" -eq 0 ]; then
  if ! command -v npm >/dev/null 2>&1; then
    fail "npm is required for frontend checks"
  fi
  info "Running Desktop npm typecheck"
  npm --prefix textlingo-desktop run typecheck
  info "Running Desktop Vitest"
  npm --prefix textlingo-desktop test -- --run
else
  warn "Skipped frontend npm checks"
fi

info "PR-4.5 learning-state checks completed"
