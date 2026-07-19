#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: bash script/verify_pr4_6_legacy_imports.sh [--start-db] [--skip-frontend]

Checks the PR-4.6 legacy JSON import implementation.

Default mode:
  - verifies expected legacy import files and route registrations
  - validates docker-compose.dev.yml when Docker Compose is available
  - runs backend cargo fmt/check/test
  - runs Desktop cargo check/test
  - runs Desktop npm typecheck/test/build:all unless --skip-frontend is passed
  - runs PostgreSQL integration tests only when OPENKOTO_TEST_DATABASE_URL is set

Options:
  --start-db       Start the PostgreSQL development container and use it as OPENKOTO_TEST_DATABASE_URL.
  --skip-frontend  Skip npm typecheck, Vitest, and build:all.

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
  printf '[pr4.6] %s\n' "$1"
}

warn() {
  printf '[pr4.6][warn] %s\n' "$1" >&2
}

fail() {
  printf '[pr4.6][error] %s\n' "$1" >&2
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

require_file "openkoto-backend/src/legacy_imports.rs"
require_file "openkoto-backend/tests/legacy_imports.rs"
require_file "openkoto-backend/migrations/20260707000500_legacy_imports.sql"
require_file "textlingo-desktop/src-tauri/src/legacy_import.rs"
require_file "textlingo-desktop/src-tauri/src/backend_client.rs"
require_file "textlingo-desktop/src-tauri/src/lib.rs"

for expected in \
  "CREATE TABLE legacy_import_batches" \
  "CREATE TABLE legacy_import_items" \
  "request_sha256 TEXT NOT NULL" \
  "schema_version TEXT NOT NULL" \
  "legacy_import_batches_user_id_id_unique"; do
  if ! grep -q "$expected" openkoto-backend/migrations/20260707000500_legacy_imports.sql; then
    fail "Legacy import migration missing: $expected"
  fi
done

for expected in \
  "post(legacy_imports::create_legacy_import)" \
  "get(legacy_imports::get_legacy_import)" \
  "AuthenticatedUser" \
  "legacy_import_payload_conflict"; do
  if ! grep -R -q "$expected" openkoto-backend/src/legacy_imports.rs openkoto-backend/src/routes.rs; then
    fail "Legacy import backend code missing: $expected"
  fi
done

for expected in \
  "LegacyImportRequest" \
  "create_legacy_import" \
  "run_legacy_import_cmd" \
  "get_legacy_import_cmd"; do
  if ! grep -R -q "$expected" textlingo-desktop/src-tauri/src/backend_client.rs textlingo-desktop/src-tauri/src/legacy_import.rs textlingo-desktop/src-tauri/src/lib.rs; then
    fail "Desktop legacy import code missing: $expected"
  fi
done

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
  info "Running Desktop build:all"
  npm --prefix textlingo-desktop run build:all
else
  warn "Skipped frontend npm checks"
fi

info "PR-4.6 legacy import checks completed"
