#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RUN_FULL=0
RUN_PR9=0

usage() {
  cat <<'USAGE'
Usage: bash script/verify_pr10_learning_domain.sh [--full] [--run-pr9]

Checks the PR-10 canonical learning domain, activity history, Desktop bridge,
learning workbench, and release-version contracts.

Options:
  --full     Run Backend, Desktop, frontend, build, and Playwright verification.
  --run-pr9  Chain the PR-9/PR-8/PR-7 regression gates.
  -h, --help Show this help.
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --full) RUN_FULL=1 ;;
    --run-pr9) RUN_PR9=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "[PR-10] ERROR: unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

info() {
  echo "[PR-10] $*"
}

fail() {
  echo "[PR-10] ERROR: $*" >&2
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

MIGRATION="openkoto-backend/migrations/20260714001100_learning_domain_activity.sql"
BACKEND="openkoto-backend/src/learning_items.rs"
ACTIVITY="openkoto-backend/src/learning_activity.rs"
BACKEND_TEST="openkoto-backend/tests/pr10_learning_domain.rs"
WORKBENCH="textlingo-desktop/src/components/features/LearningWorkbench.tsx"
WORKBENCH_API="textlingo-desktop/src/features/learning/api.ts"

for path in \
  "$MIGRATION" \
  "$BACKEND" \
  "$ACTIVITY" \
  "$BACKEND_TEST" \
  "$WORKBENCH" \
  "$WORKBENCH_API" \
  docs/architecture/pr10-learning-domain-and-local-review.md \
  textlingo-desktop/src-tauri/src/backend_client.rs \
  textlingo-desktop/src-tauri/src/commands.rs \
  textlingo-desktop/src/app/navigation.ts \
  textlingo-desktop/src/app/routes.tsx \
  textlingo-desktop/e2e/learning-workbench-flow.spec.ts; do
  require_file "$path"
done

info "checking shell syntax and source formatting"
bash -n script/verify_pr10_learning_domain.sh
git diff --check

info "checking canonical schema and source preservation"
for pattern in \
  'quality_flags JSONB' \
  'status_before_archive' \
  'merged_into_id' \
  'source_material_title_snapshot' \
  'ON DELETE SET NULL \(material_id\)' \
  'CREATE TABLE IF NOT EXISTS word_pack_learning_items' \
  'CREATE TABLE IF NOT EXISTS learning_activity_events' \
  'payload_sha256' \
  'learning_activity_events_user_idempotency_idx'; do
  require_pattern "$pattern" "$MIGRATION"
done

info "checking state machine, compatibility migration, and activity API"
for pattern in \
  'validate_status_transition_for_item' \
  'accepted_learning_item_type_immutable' \
  'learning_item_merge_accepted_source' \
  'bulk_organize_learning_items' \
  'migrate_legacy_learning_items' \
  'canonicalize_favorite_vocabulary_tx' \
  'word_pack_learning_items'; do
  require_pattern "$pattern" "$BACKEND"
done
for pattern in \
  'learning_activity_idempotency_conflict' \
  'record_read_event_tx' \
  'timezone_offset_minutes' \
  'get_daily_learning_review' \
  'get_material_learning_review' \
  'record_local_preview'; do
  require_pattern "$pattern" "$ACTIVITY"
done
for route in \
  '/learning-items/bulk-organize' \
  '/learning-items/compatibility-migration' \
  '/learning-items/\{id\}/local-preview' \
  '/learning-activity-events' \
  '/learning-review/daily' \
  '/materials/\{id\}/learning-review'; do
  require_pattern "$route" openkoto-backend/src/routes.rs
done

info "checking Desktop bridge and workbench"
for command in \
  bulk_organize_learning_items_cmd \
  migrate_legacy_learning_items_cmd \
  list_learning_activity_events_cmd \
  get_daily_learning_review_cmd \
  get_material_learning_review_cmd \
  record_local_preview_cmd; do
  require_pattern "$command" textlingo-desktop/src-tauri/src/commands.rs
  require_pattern "$command" textlingo-desktop/src-tauri/src/lib.rs
done
for pattern in \
  '批量接受' \
  '合并到当前项' \
  '待人工核验' \
  '本地预习' \
  '兼容数据检查' \
  'onNavigateToSource'; do
  require_pattern "$pattern" "$WORKBENCH"
done
require_pattern 'id: "learning"' textlingo-desktop/src/app/navigation.ts
require_pattern 'activeScreen === "learning"' textlingo-desktop/src/app/routes.tsx

info "checking 0.8.0 release version"
EXPECTED_VERSION=0.8.0 bash script/verify_release_version.sh

if [ "$RUN_FULL" -eq 1 ]; then
  info "running Backend checks and tests"
  cargo fmt --manifest-path openkoto-backend/Cargo.toml --all -- --check
  cargo check --manifest-path openkoto-backend/Cargo.toml
  cargo test --manifest-path openkoto-backend/Cargo.toml --no-fail-fast

  info "running Desktop Rust checks and tests"
  npm --prefix textlingo-desktop/agent-worker run build
  cargo fmt --manifest-path textlingo-desktop/src-tauri/Cargo.toml --all -- --check
  cargo check --manifest-path textlingo-desktop/src-tauri/Cargo.toml
  cargo test --manifest-path textlingo-desktop/src-tauri/Cargo.toml --lib

  info "running frontend typecheck, tests, and production build"
  npm --prefix textlingo-desktop run typecheck
  npm --prefix textlingo-desktop run test -- --run
  npm --prefix textlingo-desktop run build

  info "running learning candidate and workbench Playwright flows"
  (
    cd textlingo-desktop
    CI=1 npm exec -- playwright test \
      e2e/learning-candidate-flow.spec.ts \
      e2e/learning-workbench-flow.spec.ts \
      --config playwright.config.ts
  )
fi

if [ "$RUN_PR9" -eq 1 ]; then
  info "running PR-9 and earlier regression gates"
  bash script/verify_pr9_annotations.sh --run-pr8
fi

info "PR-10 learning-domain guardrails passed"
