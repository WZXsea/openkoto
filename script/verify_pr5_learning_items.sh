#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: bash script/verify_pr5_learning_items.sh [--start-db] [--skip-frontend] [--skip-e2e]

Checks the PR-5 local learning item and candidate inbox implementation.

Default mode:
  - verifies expected PR-5 backend, desktop bridge, frontend, and e2e files
  - validates docker-compose.dev.yml when Docker Compose is available
  - runs backend cargo fmt/check/test
  - runs Desktop cargo check/test
  - runs Desktop npm typecheck/test/build:all unless --skip-frontend is passed
  - runs Playwright learning candidate flow unless --skip-e2e or --skip-frontend is passed
  - runs PostgreSQL integration tests only when OPENKOTO_TEST_DATABASE_URL is set

Options:
  --start-db       Start the PostgreSQL development container and use it as OPENKOTO_TEST_DATABASE_URL.
  --skip-frontend  Skip npm typecheck, Vitest, build:all, and e2e.
  --skip-e2e       Skip browser e2e only.

Environment:
  OPENKOTO_POSTGRES_PORT  Host port for the dev PostgreSQL container. Defaults to 5433.
USAGE
}

START_DB=0
SKIP_FRONTEND=0
SKIP_E2E=0
for arg in "$@"; do
  case "$arg" in
    --start-db)
      START_DB=1
      ;;
    --skip-frontend)
      SKIP_FRONTEND=1
      SKIP_E2E=1
      ;;
    --skip-e2e)
      SKIP_E2E=1
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
VITE_PID=""

info() {
  printf '[pr5] %s\n' "$1"
}

warn() {
  printf '[pr5][warn] %s\n' "$1" >&2
}

fail() {
  printf '[pr5][error] %s\n' "$1" >&2
  exit 1
}

cleanup() {
  if [ -n "$VITE_PID" ] && kill -0 "$VITE_PID" >/dev/null 2>&1; then
    kill "$VITE_PID" >/dev/null 2>&1 || true
    wait "$VITE_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

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

require_file "openkoto-backend/migrations/20260707000600_learning_items.sql"
require_file "openkoto-backend/src/learning_items.rs"
require_file "openkoto-backend/tests/learning_items.rs"
require_file "textlingo-desktop/src/components/features/LearningCandidateBox.tsx"
require_file "textlingo-desktop/src/components/features/LearningCandidateBox.test.tsx"
require_file "textlingo-desktop/src/lib/learningItems.ts"
require_file "textlingo-desktop/src/lib/learningItems.test.ts"
require_file "textlingo-desktop/e2e/learning-candidate-flow.spec.ts"

for expected in \
  "CREATE TABLE learning_items" \
  "learning_items_user_dedupe_key_idx" \
  "item_type IN ('word', 'phrase', 'sentence', 'grammar')" \
  "status IN ('candidate', 'accepted', 'rejected', 'archived')"; do
  if ! grep -q "$expected" openkoto-backend/migrations/20260707000600_learning_items.sql; then
    fail "Learning items migration missing: $expected"
  fi
done

for expected in \
  "create_learning_item_from_selection" \
  "bulk_learning_item_status" \
  "dedupe_key" \
  "AuthenticatedUser"; do
  if ! grep -R -q "$expected" openkoto-backend/src/learning_items.rs openkoto-backend/src/routes.rs; then
    fail "Learning items backend code missing: $expected"
  fi
done

for expected in \
  "create_learning_item_from_selection_cmd" \
  "LearningCandidateBox" \
  "createLearningItemFromSelection" \
  "data-reader-segment-id"; do
  if ! grep -R -q "$expected" textlingo-desktop/src-tauri/src textlingo-desktop/src; then
    fail "Desktop learning item code missing: $expected"
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

  if [ "$SKIP_E2E" -eq 0 ]; then
    info "Starting Vite server for Playwright"
    npm --prefix textlingo-desktop run dev -- --host 127.0.0.1 > /tmp/openkoto-pr5-vite.log 2>&1 &
    VITE_PID="$!"

    for _ in $(seq 1 40); do
      if curl -fsS http://127.0.0.1:1420/ >/dev/null 2>&1; then
        info "Vite server is ready"
        break
      fi
      if ! kill -0 "$VITE_PID" >/dev/null 2>&1; then
        cat /tmp/openkoto-pr5-vite.log >&2 || true
        fail "Vite server exited before becoming ready"
      fi
      sleep 1
    done

    if ! curl -fsS http://127.0.0.1:1420/ >/dev/null 2>&1; then
      cat /tmp/openkoto-pr5-vite.log >&2 || true
      fail "Vite server did not become ready"
    fi

    info "Running Playwright learning candidate e2e"
    (
      cd textlingo-desktop
      npm exec -- playwright test e2e/learning-candidate-flow.spec.ts --config playwright.config.ts
    )
  else
    warn "Skipped Playwright e2e"
  fi
else
  warn "Skipped frontend npm checks"
fi

info "PR-5 learning item checks completed"
