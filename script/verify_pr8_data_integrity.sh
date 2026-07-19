#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RUN_PR7=0

usage() {
  cat <<'USAGE'
Usage: bash script/verify_pr8_data_integrity.sh [--run-pr7]

Verifies the PR-8 data-integrity guardrails that do not require a running
database or packaged application.

Options:
  --run-pr7  Run the full PR-7 material-workbench verification after the
             PR-8 static gates. This may require Docker, Node dependencies,
             Rust dependencies, and Playwright browsers.
  -h, --help Show this help.
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --run-pr7) RUN_PR7=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "[PR-8] ERROR: unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

info() {
  echo "[PR-8] $*"
}

fail() {
  echo "[PR-8] ERROR: $*" >&2
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

forbid_pattern() {
  local pattern="$1"
  local file="$2"
  if rg -q -- "$pattern" "$file"; then
    fail "forbidden pattern '$pattern' found in $file"
  fi
}

line_number() {
  local pattern="$1"
  local file="$2"
  rg -n -m 1 -- "$pattern" "$file" | cut -d: -f1
}

PR7_SCRIPT="script/verify_pr7_material_workbench.sh"
OWNERSHIP_DOC="docs/architecture/pr8-data-ownership-and-backup.md"

require_file "$PR7_SCRIPT"
require_file "$OWNERSHIP_DOC"

info "checking shell syntax"
bash -n "$PR7_SCRIPT"
bash -n "script/verify_pr8_data_integrity.sh"

info "checking PR-7 verification order"
worker_build_line="$(line_number 'npm --prefix textlingo-desktop run build:agent-worker' "$PR7_SCRIPT")"
desktop_check_line="$(line_number 'cargo check --locked --manifest-path textlingo-desktop/src-tauri/Cargo.toml' "$PR7_SCRIPT")"
[ -n "$worker_build_line" ] || fail "PR-7 verifier has no explicit agent-worker build step"
[ -n "$desktop_check_line" ] || fail "PR-7 verifier has no Desktop cargo check step"
[ "$worker_build_line" -lt "$desktop_check_line" ] \
  || fail "agent-worker must be built before Desktop cargo check"

info "checking data-ownership implementation anchors"
for path in \
  openkoto-backend/src/config.rs \
  openkoto-backend/src/database.rs \
  openkoto-backend/src/files.rs \
  openkoto-backend/migrations/20260707000300_materials_files.sql \
  openkoto-backend/migrations/20260707000400_learning_state.sql \
  openkoto-backend/migrations/20260713000900_learning_item_acceptance.sql \
  textlingo-desktop/src-tauri/src/data_backup.rs \
  textlingo-desktop/src-tauri/tests/agent_task_checkpoint_integrity_test.rs \
  textlingo-desktop/src-tauri/src/packaged_backend.rs \
  textlingo-desktop/src-tauri/src/storage.rs \
  textlingo-desktop/src-tauri/src/logging.rs \
  textlingo-desktop/src-tauri/src/source_locator.rs \
  textlingo-desktop/src/features/reader/sourceLocator.ts \
  textlingo-desktop/src/components/features/LearningCandidateBox.tsx \
  textlingo-desktop/src-tauri/src/video_server.rs; do
  require_file "$path"
done

require_pattern 'OPENKOTO_JWT_SECRET' openkoto-backend/src/config.rs
require_pattern 'OPENKOTO_FILE_STORAGE_DIR' openkoto-backend/src/config.rs
require_pattern 'backend_dir\.join\("files"\)' textlingo-desktop/src-tauri/src/packaged_backend.rs
require_pattern 'backend_dir\.join\("postgres-data"\)' textlingo-desktop/src-tauri/src/packaged_backend.rs
require_pattern 'backend_dir\.join\("jwt_secret"\)' textlingo-desktop/src-tauri/src/packaged_backend.rs
require_pattern 'BACKUP_FORMAT_VERSION' textlingo-desktop/src-tauri/src/data_backup.rs
require_pattern 'file_storage_entries' textlingo-desktop/src-tauri/src/data_backup.rs
require_pattern 'create_upgrade_backup_if_needed' textlingo-desktop/src-tauri/src/packaged_backend.rs
require_pattern 'SOURCE_LOCATOR_VERSION' textlingo-desktop/src-tauri/src/source_locator.rs
require_pattern 'content_sha256' textlingo-desktop/src-tauri/src/source_locator.rs
require_pattern 'SOURCE_LOCATOR_VERSION' textlingo-desktop/src/features/reader/sourceLocator.ts
require_pattern 'parseSourceLocator' textlingo-desktop/src/features/reader/sourceLocator.ts
require_pattern 'const CONFIG_FILE: &str = "config.json"' textlingo-desktop/src-tauri/src/storage.rs
require_pattern 'const WORKER_CHECKPOINTS_DIR: &str = "agent_worker_checkpoints"' textlingo-desktop/src-tauri/src/storage.rs
require_pattern 'const LEGACY_AGENT_TASKS_DIR: &str = "agent_tasks"' textlingo-desktop/src-tauri/src/storage.rs
require_pattern 'const LEGACY_ARTIFACTS_DIR: &str = "artifacts/articles"' textlingo-desktop/src-tauri/src/storage.rs
require_pattern 'app_data_dir>/logs/openkoto.log' textlingo-desktop/src-tauri/src/logging.rs
require_pattern 'app_data_dir\.join\("videos"\)' textlingo-desktop/src-tauri/src/video_server.rs
require_pattern 'app_data_dir\.join\("books"\)' textlingo-desktop/src-tauri/src/video_server.rs
require_pattern 'CREATE TABLE files' openkoto-backend/migrations/20260707000300_materials_files.sql
require_pattern 'CREATE TABLE agent_tasks' openkoto-backend/migrations/20260707000400_learning_state.sql
require_pattern 'CREATE TABLE artifacts' openkoto-backend/migrations/20260707000400_learning_state.sql
require_pattern 'learning_item_id' openkoto-backend/migrations/20260713000900_learning_item_acceptance.sql
require_pattern 'favorite_vocabularies_learning_item_fk' openkoto-backend/migrations/20260713000900_learning_item_acceptance.sql
require_pattern 'favorite_grammars_learning_item_fk' openkoto-backend/migrations/20260713000900_learning_item_acceptance.sql
require_pattern '/learning-items/\{id\}/accept' openkoto-backend/src/routes.rs
require_pattern 'state\.pool\.begin\(\)' openkoto-backend/src/learning_items.rs
require_pattern 'atomic_acceptance_required' openkoto-backend/src/learning_items.rs
require_pattern 'accept_learning_item_cmd' textlingo-desktop/src-tauri/src/commands.rs
require_pattern 'acceptLearningItem' textlingo-desktop/src/components/features/LearningCandidateBox.tsx
forbid_pattern 'add_favorite_vocabulary_cmd' textlingo-desktop/src/components/features/LearningCandidateBox.tsx

info "checking Backend-first agent task and artifact ownership"
require_pattern 'persist_agent_task_backend' textlingo-desktop/src-tauri/src/commands.rs
require_pattern 'persist_artifact_backend' textlingo-desktop/src-tauri/src/commands.rs
require_pattern 'recover_worker_checkpoints_from_backend' textlingo-desktop/src-tauri/src/agent_worker.rs
require_pattern 'recover_worker_checkpoints_from_backend' textlingo-desktop/src-tauri/src/lib.rs
require_pattern 'backend_failure_does_not_create_worker_checkpoint' textlingo-desktop/src-tauri/tests/agent_task_checkpoint_integrity_test.rs
require_pattern 'restart_marks_only_worker_checkpoint_for_backend_recovery' textlingo-desktop/src-tauri/tests/agent_task_checkpoint_integrity_test.rs
forbid_pattern 'persist_agent_task_backend_and_local' textlingo-desktop/src-tauri/src/commands.rs

info "checking ownership document coverage"
for heading in \
  '事实源与责任边界' \
  '备份范围' \
  '恢复顺序' \
  '删除策略' \
  'Secret 处理' \
  'PostgreSQL data' \
  'backend/files' \
  'jwt_secret' \
  'App config' \
  'agent_tasks' \
  'artifacts' \
  '日志' \
  'books' \
  'videos'; do
  require_pattern "$heading" "$OWNERSHIP_DOC"
done

git diff --check

if [ "$RUN_PR7" -eq 1 ]; then
  info "running PR-7 verification"
  bash "$PR7_SCRIPT"
fi

info "PR-8 data-integrity guardrails passed"
