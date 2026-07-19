#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

SKIP_DATABASE=0
SKIP_FRONTEND=0
SKIP_E2E=0
BUILD_APP=0

usage() {
  cat <<'USAGE'
Usage: bash script/verify_pr7_material_workbench.sh [options]

Options:
  --skip-database   Skip Docker PostgreSQL and database integration tests.
  --skip-frontend   Skip frontend typecheck, tests, and build.
  --skip-e2e        Skip Playwright material-workbench tests.
  --build-app       Build and verify the packaged macOS app and DMG.
  -h, --help        Show this help.
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --skip-database) SKIP_DATABASE=1 ;;
    --skip-frontend) SKIP_FRONTEND=1 ;;
    --skip-e2e) SKIP_E2E=1 ;;
    --build-app) BUILD_APP=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

info() {
  echo "[PR-7] $*"
}

fail() {
  echo "[PR-7] ERROR: $*" >&2
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
    fail "Docker Compose is required unless --skip-database is used"
  fi
}

cleanup_verify_database() {
  :
}

prepare_agent_worker() {
  info "building agent worker before Desktop Rust checks"
  npm --prefix textlingo-desktop run build:agent-worker
  for worker_file in index.js assistantTask.js mindMapTask.js piRuntime.js protocol.js runtime.js; do
    test -f "textlingo-desktop/agent-worker/dist/${worker_file}" \
      || fail "agent worker build is missing textlingo-desktop/agent-worker/dist/${worker_file}"
  done
}

require_file "openkoto-backend/migrations/20260710000700_material_library.sql"
require_file "openkoto-backend/migrations/20260711000800_material_text_file_source_type.sql"
require_file "openkoto-backend/src/material_library.rs"
require_file "openkoto-backend/tests/material_library.rs"
require_file "textlingo-desktop/src/features/materials/types.ts"
require_file "textlingo-desktop/src/features/materials/selectors.ts"
require_file "textlingo-desktop/src/features/materials/selectors.test.ts"
require_file "textlingo-desktop/src/features/reader/readingProgress.ts"
require_file "textlingo-desktop/src/features/reader/readingProgress.test.tsx"
require_file "textlingo-desktop/src/components/features/ArticleList.test.tsx"
require_file "textlingo-desktop/e2e/material-workbench-flow.spec.ts"

MIGRATION="openkoto-backend/migrations/20260710000700_material_library.sql"
for table in material_import_jobs material_tags material_tag_links reading_progress; do
  require_pattern "CREATE TABLE ${table}" "$MIGRATION"
done
for field in source_kind source_uri input_hash error_code error_message result_material_id preview reader_kind locator progress_ratio last_opened_at archived_at; do
  require_pattern "$field" "$MIGRATION"
done
for status in queued validating parsing preview_ready committing succeeded failed_retryable failed_terminal cancelled; do
  require_pattern "$status" "$MIGRATION"
done

require_pattern '127\.0\.0\.1:19421' "textlingo-desktop/src-tauri/src/packaged_backend.rs"
require_pattern 'duplicate-check' "openkoto-backend/src/routes.rs"
require_pattern 'material-import-jobs' "openkoto-backend/src/routes.rs"
require_pattern 'reading-progress' "openkoto-backend/src/routes.rs"
require_pattern 'material-tags' "openkoto-backend/src/routes.rs"
require_pattern 'preview_material_import_cmd' "textlingo-desktop/src-tauri/src/commands.rs"
require_pattern 'material_library_resume_import_job_cmd' "textlingo-desktop/src-tauri/src/commands.rs"
require_pattern 'text_file' "openkoto-backend/migrations/20260711000800_material_text_file_source_type.sql"
require_pattern 'ReadingProgressUpdate' "textlingo-desktop/src/features/reader/readingProgress.ts"
require_pattern 'filterAndSortMaterials' "textlingo-desktop/src/components/features/ArticleList.tsx"
bash script/verify_release_version.sh

if [ "$SKIP_DATABASE" -eq 0 ]; then
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
  info "using PostgreSQL host port ${OPENKOTO_POSTGRES_PORT}"
  info "starting PostgreSQL development container"
  compose -f docker-compose.dev.yml up -d openkoto-postgres
  container_id="$(compose -f docker-compose.dev.yml ps -q openkoto-postgres)"
  [ -n "$container_id" ] || fail "openkoto-postgres container was not created"

  info "waiting for PostgreSQL health"
  for _ in $(seq 1 60); do
    status="$(docker inspect -f '{{.State.Health.Status}}' "$container_id" 2>/dev/null || true)"
    [ "$status" = "healthy" ] && break
    sleep 1
  done
  status="$(docker inspect -f '{{.State.Health.Status}}' "$container_id" 2>/dev/null || true)"
  [ "$status" = "healthy" ] || fail "PostgreSQL did not become healthy; status=$status"
  postgres_port="$(docker inspect -f '{{(index (index .NetworkSettings.Ports "5432/tcp") 0).HostPort}}' "$container_id")"
  verify_database="openkoto_pr7_verify_$$"
  docker exec "$container_id" createdb -U openkoto "$verify_database"
  cleanup_verify_database() {
    docker exec "$container_id" dropdb -U openkoto --if-exists "$verify_database" >/dev/null 2>&1 || true
  }
  trap cleanup_verify_database EXIT
  export OPENKOTO_TEST_DATABASE_URL="postgres://openkoto:openkoto_dev_password@127.0.0.1:${postgres_port}/${verify_database}"
else
  info "database integration tests skipped"
  unset OPENKOTO_TEST_DATABASE_URL || true
fi

info "checking backend formatting"
cargo fmt --manifest-path openkoto-backend/Cargo.toml --check
info "checking backend compilation"
cargo check --locked --manifest-path openkoto-backend/Cargo.toml
info "running backend tests"
cargo test --locked --manifest-path openkoto-backend/Cargo.toml

prepare_agent_worker
info "checking Desktop Rust compilation"
cargo check --locked --manifest-path textlingo-desktop/src-tauri/Cargo.toml
info "running Desktop Rust tests"
cargo test --locked --manifest-path textlingo-desktop/src-tauri/Cargo.toml

if [ "$SKIP_FRONTEND" -eq 0 ]; then
  info "checking frontend types"
  npm --prefix textlingo-desktop run typecheck
  info "running frontend tests"
  npm --prefix textlingo-desktop test -- --run
  info "building frontend (agent worker was prepared before Desktop Rust checks)"
  npm --prefix textlingo-desktop run build

  if [ "$SKIP_E2E" -eq 0 ]; then
    info "running Playwright material-workbench flow"
    (
      cd textlingo-desktop
      CI=1 npm exec -- playwright test e2e/material-workbench-flow.spec.ts --config playwright.config.ts
    )
  fi
fi

if [ "$BUILD_APP" -eq 1 ]; then
  info "building packaged app"
  npm --prefix textlingo-desktop run tauri:build:packaged
  app_path="$(find textlingo-desktop/src-tauri/target -path '*/release/bundle/macos/*.app' -type d -print -quit)"
  dmg_path="$(find textlingo-desktop/src-tauri/target -path '*/release/bundle/dmg/*.dmg' -type f -print -quit)"
  [ -n "$app_path" ] || fail "packaged build completed without a macOS app"
  [ -n "$dmg_path" ] || fail "packaged build completed without a DMG"
  app_path="$(cd "$app_path" && pwd)"

  info "verifying packaged runtime resources"
  test -x "$app_path/Contents/Resources/backend/openkoto-backend" || fail "backend sidecar is missing from app bundle"
  test -x "$app_path/Contents/Resources/postgres/bin/postgres" || fail "PostgreSQL is missing from app bundle"
  test -x "$app_path/Contents/Resources/postgres/bin/initdb" || fail "initdb is missing from app bundle"
  test -x "$app_path/Contents/Resources/node/bin/node" || fail "Node.js is missing from app bundle"
  test -f "$app_path/Contents/Resources/agent-worker/package.json" || fail "agent worker package is missing from app bundle"
  test -f "$app_path/Contents/Resources/agent-worker/dist/index.js" || fail "agent worker build is missing from app bundle"
  test -f "$app_path/Contents/Resources/agent-worker/dist/piRuntime.js" || fail "Pi runtime build is missing from app bundle"
  app_node="$app_path/Contents/Resources/node/bin/node"
  app_worker="$app_path/Contents/Resources/agent-worker"
  "$app_node" -e '
    const [major, minor] = process.versions.node.split(".").map(Number);
    if (major < 22 || (major === 22 && minor < 19)) process.exit(1);
  ' || fail "packaged Node must satisfy Pi runtime requirement >=22.19.0"
  (
    cd "$app_worker"
    "$app_node" --input-type=module -e '
      await import("@earendil-works/pi-agent-core");
      await import("@earendil-works/pi-ai");
    '
  ) || fail "packaged Node cannot resolve Pi runtime modules"

  info "verifying DMG image and bundled app"
  hdiutil verify "$dmg_path" >/dev/null
  mount_dir="$(mktemp -d)"
  mounted=0
  cleanup_dmg_mount() {
    if [ "$mounted" -eq 1 ]; then
      hdiutil detach "$mount_dir" >/dev/null 2>&1 || true
    fi
    rmdir "$mount_dir" >/dev/null 2>&1 || true
  }
  trap 'cleanup_dmg_mount; cleanup_verify_database' EXIT
  hdiutil attach "$dmg_path" -mountpoint "$mount_dir" -nobrowse -readonly >/dev/null
  mounted=1
  dmg_app="$(find "$mount_dir" -maxdepth 2 -type d -name '*.app' -print -quit)"
  [ -n "$dmg_app" ] || fail "DMG does not contain an app bundle"
  test -x "$dmg_app/Contents/Resources/backend/openkoto-backend" || fail "DMG app is missing backend sidecar"
  test -x "$dmg_app/Contents/Resources/postgres/bin/postgres" || fail "DMG app is missing PostgreSQL"
  test -x "$dmg_app/Contents/Resources/node/bin/node" || fail "DMG app is missing Node.js"
  test -f "$dmg_app/Contents/Resources/agent-worker/dist/index.js" || fail "DMG app is missing agent worker build"
  test -f "$dmg_app/Contents/Resources/agent-worker/dist/piRuntime.js" || fail "DMG app is missing Pi runtime build"
  dmg_node="$dmg_app/Contents/Resources/node/bin/node"
  dmg_worker="$dmg_app/Contents/Resources/agent-worker"
  "$dmg_node" -e '
    const [major, minor] = process.versions.node.split(".").map(Number);
    if (major < 22 || (major === 22 && minor < 19)) process.exit(1);
  ' || fail "DMG bundled Node must satisfy Pi runtime requirement >=22.19.0"
  (
    cd "$dmg_worker"
    "$dmg_node" --input-type=module -e '
      await import("@earendil-works/pi-agent-core");
      await import("@earendil-works/pi-ai");
    '
  ) || fail "DMG bundled Node cannot resolve Pi runtime modules"
  cleanup_dmg_mount
  mounted=0
fi

git diff --check
info "PR-7 material workbench checks completed"
