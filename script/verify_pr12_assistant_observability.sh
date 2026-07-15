#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RUN_FULL=0
RUN_PR11=0
START_DB=0

usage() {
  cat <<'USAGE'
Usage: bash script/verify_pr12_assistant_observability.sh [--full] [--run-pr11] [--start-db]

Checks PR-12 Assistant task observability, timeline, cancellation, retry,
artifact viewing, evidence navigation, and action-registry boundaries.

Options:
  --full      Run Backend, worker, Desktop, frontend, build, and Playwright tests.
  --run-pr11  Chain PR-11 and earlier regression guardrails.
  --start-db  Start Docker PostgreSQL and run integration tests in an isolated database.
  -h, --help  Show this help.
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --full) RUN_FULL=1 ;;
    --run-pr11) RUN_PR11=1 ;;
    --start-db) START_DB=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "[PR-12] ERROR: unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

info() {
  echo "[PR-12] $*"
}

fail() {
  echo "[PR-12] ERROR: $*" >&2
  exit 1
}

require_file() {
  [ -f "$1" ] || fail "missing required file: $1"
}

require_pattern() {
  local pattern="$1"
  local file="$2"
  rg -q -- "$pattern" "$file" || fail "missing pattern '$pattern' in $file"
}

compose() {
  if docker compose version >/dev/null 2>&1; then
    docker compose "$@"
  elif command -v docker-compose >/dev/null 2>&1; then
    docker-compose "$@"
  else
    fail "Docker Compose is required for --start-db"
  fi
}

cleanup_verify_database() {
  :
}

if [ "$START_DB" -eq 1 ]; then
  if [ -z "${OPENKOTO_POSTGRES_PORT:-}" ]; then
    OPENKOTO_POSTGRES_PORT="$(python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)"
    export OPENKOTO_POSTGRES_PORT
  fi
  info "starting PostgreSQL verification service on host port ${OPENKOTO_POSTGRES_PORT}"
  compose -f docker-compose.dev.yml up -d openkoto-postgres
  container_id="$(compose -f docker-compose.dev.yml ps -q openkoto-postgres)"
  [ -n "$container_id" ] || fail "openkoto-postgres container was not created"
  for _ in $(seq 1 60); do
    status="$(docker inspect -f '{{.State.Health.Status}}' "$container_id" 2>/dev/null || true)"
    [ "$status" = "healthy" ] && break
    sleep 1
  done
  status="$(docker inspect -f '{{.State.Health.Status}}' "$container_id" 2>/dev/null || true)"
  [ "$status" = "healthy" ] || fail "PostgreSQL did not become healthy; status=$status"
  postgres_port="$(docker inspect -f '{{(index (index .NetworkSettings.Ports "5432/tcp") 0).HostPort}}' "$container_id")"
  verify_database="openkoto_pr12_verify_$$"
  docker exec "$container_id" createdb -U openkoto "$verify_database"
  cleanup_verify_database() {
    docker exec "$container_id" dropdb -U openkoto --if-exists "$verify_database" >/dev/null 2>&1 || true
  }
  trap cleanup_verify_database EXIT
  export OPENKOTO_TEST_DATABASE_URL="postgres://openkoto:openkoto_dev_password@127.0.0.1:${postgres_port}/${verify_database}"
elif [ "$RUN_FULL" -eq 1 ] && [ -z "${OPENKOTO_TEST_DATABASE_URL:-}" ]; then
  info "warning: OPENKOTO_TEST_DATABASE_URL is unset; PostgreSQL integration tests will be skipped"
fi

for path in \
  openkoto-backend/migrations/20260715001200_agent_observability.sql \
  openkoto-backend/src/assistant.rs \
  openkoto-backend/tests/agent_observability.rs \
  textlingo-desktop/src-tauri/src/assistant/actions.rs \
  textlingo-desktop/src-tauri/src/assistant/commands.rs \
  textlingo-desktop/src-tauri/src/assistant/dto.rs \
  textlingo-desktop/src-tauri/src/assistant/service.rs \
  textlingo-desktop/src/features/assistant/api.ts \
  textlingo-desktop/src/features/assistant/state.ts \
  textlingo-desktop/src/features/assistant/types.ts \
  textlingo-desktop/src/components/features/AssistantTaskCenter.tsx \
  textlingo-desktop/src/components/features/AssistantArtifactViewer.tsx \
  textlingo-desktop/src/components/features/AssistantTaskCenter.test.tsx \
  textlingo-desktop/src/components/features/AssistantArtifactViewer.test.tsx \
  docs/architecture/pr12-assistant-observability.md; do
  require_file "$path"
done

info "checking syntax, formatting, locales, and version"
bash -n script/verify_pr12_assistant_observability.sh
git diff --check
node -e 'for (const file of ["en", "ja", "zh"]) JSON.parse(require("fs").readFileSync(`textlingo-desktop/src/locales/${file}.json`, "utf8"))'
EXPECTED_VERSION="${EXPECTED_VERSION:-0.10.0}" bash script/verify_release_version.sh

info "checking Backend task, timeline, retry, and action contracts"
for pattern in \
  'agent_tasks_status_valid' \
  'agent_task_events' \
  'assistant_action_audits' \
  'retry_of_task_id' \
  'input_snapshot' \
  'output_version'; do
  require_pattern "$pattern" openkoto-backend/migrations/20260715001200_agent_observability.sql
done
for pattern in \
  'terminal_agent_task_immutable' \
  'cancel_agent_task' \
  'retry_agent_task' \
  'ingest_task_timeline' \
  'audit_task_action' \
  'external_write_forbidden'; do
  require_pattern "$pattern" openkoto-backend/src/assistant.rs
done

info "checking worker cancellation and Desktop Assistant bridge"
require_pattern 'method: z.literal\("agent.cancel"\)' textlingo-desktop/agent-worker/src/protocol.ts
require_pattern 'AbortController' textlingo-desktop/agent-worker/src/index.ts
require_pattern 'session.abort' textlingo-desktop/agent-worker/src/mindMapTask.ts
for command in \
  assistant_task_list_cmd \
  assistant_task_detail_cmd \
  assistant_task_timeline_cmd \
  assistant_task_cancel_cmd \
  assistant_task_retry_cmd \
  assistant_task_artifacts_cmd; do
  require_pattern "$command" textlingo-desktop/src-tauri/src/assistant/commands.rs
  require_pattern "$command" textlingo-desktop/src-tauri/src/lib.rs
done
for pattern in 'open_material' 'open_source' 'external_or_write_action_forbidden'; do
  require_pattern "$pattern" textlingo-desktop/src-tauri/src/assistant/actions.rs
done
require_pattern 'execute_registered_assistant_action' textlingo-desktop/src-tauri/src/assistant/actions.rs
require_pattern 'execute_registered_assistant_action' textlingo-desktop/src-tauri/src/commands.rs
require_pattern 'execute_registered_assistant_action' textlingo-desktop/src-tauri/src/agent_worker.rs
if rg -q 'extract_open_material_id' textlingo-desktop/src-tauri/src; then
  fail "legacy direct Assistant navigation bypass is still present"
fi

info "checking task center, artifacts, and source navigation"
require_pattern 'id: "assistant"' textlingo-desktop/src/app/navigation.ts
require_pattern 'AssistantTaskCenter' textlingo-desktop/src/app/routes.tsx
require_pattern '输入快照' textlingo-desktop/src/components/features/AssistantTaskCenter.tsx
require_pattern '运行时间线' textlingo-desktop/src/components/features/AssistantTaskCenter.tsx
require_pattern '产物文件不可用' textlingo-desktop/src/components/features/AssistantArtifactViewer.tsx
require_pattern 'learning_item' textlingo-desktop/src/features/assistant/types.ts

if [ "$RUN_FULL" -eq 1 ]; then
  info "running Backend checks and tests"
  cargo fmt --manifest-path openkoto-backend/Cargo.toml --all -- --check
  cargo check --manifest-path openkoto-backend/Cargo.toml
  cargo test --manifest-path openkoto-backend/Cargo.toml --no-fail-fast

  info "running agent-worker checks and tests"
  npm --prefix textlingo-desktop/agent-worker run typecheck
  npm --prefix textlingo-desktop/agent-worker run test
  npm --prefix textlingo-desktop/agent-worker run build

  info "running Desktop Rust checks and tests"
  cargo fmt --manifest-path textlingo-desktop/src-tauri/Cargo.toml --all -- --check
  cargo check --manifest-path textlingo-desktop/src-tauri/Cargo.toml
  cargo test --manifest-path textlingo-desktop/src-tauri/Cargo.toml --no-fail-fast

  info "running frontend typecheck, tests, and production build"
  npm --prefix textlingo-desktop run typecheck
  npm --prefix textlingo-desktop run test -- --run
  npm --prefix textlingo-desktop run build

  info "running Playwright workflows"
  (
    cd textlingo-desktop
    CI=1 npm exec -- playwright test --config playwright.config.ts
  )
fi

if [ "$RUN_PR11" -eq 1 ]; then
  info "running PR-11 and earlier regression guardrails"
  EXPECTED_VERSION="${EXPECTED_VERSION:-0.10.0}" bash script/verify_pr11_reading_home.sh --run-pr10
fi

info "PR-12 Assistant observability guardrails passed"
