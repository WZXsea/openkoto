#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: bash script/verify_pr4_1_backend_env.sh [--start-db]

Checks the PR-4.1 backend development environment.

Default mode is read-only:
  - verifies expected backend documentation and env files
  - validates docker-compose.dev.yml when Docker Compose is available
  - runs backend cargo checks only when openkoto-backend/Cargo.toml exists

Options:
  --start-db   Start the PostgreSQL development container and wait for health.
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

info() {
  printf '[pr4.1] %s\n' "$1"
}

warn() {
  printf '[pr4.1][warn] %s\n' "$1" >&2
}

fail() {
  printf '[pr4.1][error] %s\n' "$1" >&2
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
require_file "openkoto-backend/README.md"
require_file "openkoto-backend/VERIFICATION.md"
require_file "openkoto-backend/.env.example"
require_file "openkoto-backend/migrations/README.md"

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
fi

if [ -f "openkoto-backend/Cargo.toml" ]; then
  if ! command -v cargo >/dev/null 2>&1; then
    fail "Cargo is required when openkoto-backend/Cargo.toml exists"
  fi

  info "Running backend cargo check"
  cargo check --manifest-path openkoto-backend/Cargo.toml

  info "Running backend cargo test"
  cargo test --manifest-path openkoto-backend/Cargo.toml
else
  warn "openkoto-backend/Cargo.toml does not exist yet; skipped backend cargo checks"
fi

info "PR-4.1 backend environment checks completed"
