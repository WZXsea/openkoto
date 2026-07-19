#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RUN_PR8=0

usage() {
  cat <<'USAGE'
Usage: bash script/verify_pr9_annotations.sh [--run-pr8]

Checks the PR-9 annotation, source-location, and candidate-conversion contracts.

Options:
  --run-pr8  Run PR-8 static gates and the full PR-7 regression afterwards.
  -h, --help Show this help.
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --run-pr8) RUN_PR8=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "[PR-9] ERROR: unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

info() {
  echo "[PR-9] $*"
}

fail() {
  echo "[PR-9] ERROR: $*" >&2
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

MIGRATION="openkoto-backend/migrations/20260714001000_annotations.sql"
BACKEND="openkoto-backend/src/annotations.rs"
BACKEND_TEST="openkoto-backend/tests/annotations.rs"
WORKBENCH="textlingo-desktop/src/components/features/AnnotationWorkbench.tsx"
LOCATOR="textlingo-desktop/src/features/reader/annotationLocator.ts"

for path in \
  "$MIGRATION" \
  "$BACKEND" \
  "$BACKEND_TEST" \
  "$WORKBENCH" \
  "$LOCATOR" \
  textlingo-desktop/src/features/annotations/api.ts \
  textlingo-desktop/src-tauri/src/backend_client.rs \
  textlingo-desktop/src-tauri/src/commands.rs \
  textlingo-desktop/src/app/navigation.ts \
  textlingo-desktop/src/app/routes.tsx \
  textlingo-desktop/e2e/annotation-workbench-flow.spec.ts; do
  require_file "$path"
done

info "checking shell syntax"
bash -n script/verify_pr8_data_integrity.sh
bash -n script/verify_pr9_annotations.sh

info "checking annotation schema and ownership"
for pattern in \
  'CREATE TABLE IF NOT EXISTS annotations' \
  'annotations_user_material_fk' \
  'annotations_user_material_segment_fk' \
  'annotations_user_learning_item_fk' \
  'client_request_id' \
  'idempotency_payload_sha256' \
  'learning_item_id' \
  'locator JSONB' \
  'source_text TEXT'; do
  require_pattern "$pattern" "$MIGRATION"
done

info "checking Backend API, isolation, and conversion"
require_pattern 'list_annotations' "$BACKEND"
require_pattern 'create_annotation' "$BACKEND"
require_pattern 'patch_annotation' "$BACKEND"
require_pattern 'delete_annotation' "$BACKEND"
require_pattern 'convert.*learning_item' "$BACKEND"
require_pattern 'pool\.begin\(\)' "$BACKEND"
require_pattern 'client_request_id' "$BACKEND"
require_pattern 'user\.id' "$BACKEND"
require_pattern '/annotations/\{id\}/convert-to-learning-item' openkoto-backend/src/routes.rs

info "checking Desktop bridge and workbench"
for command in \
  list_annotations_cmd \
  create_annotation_cmd \
  update_annotation_cmd \
  delete_annotation_cmd \
  convert_annotation_to_learning_item_cmd; do
  require_pattern "$command" textlingo-desktop/src-tauri/src/commands.rs
  require_pattern "$command" textlingo-desktop/src-tauri/src/lib.rs
done
require_pattern 'AnnotationWorkbench' "$WORKBENCH"
require_pattern 'convertToLearningItem' "$WORKBENCH"
require_pattern 'onNavigate' "$WORKBENCH"
require_pattern 'id: "annotations"' textlingo-desktop/src/app/navigation.ts
require_pattern 'activeScreen === "annotations"' textlingo-desktop/src/app/routes.tsx
require_pattern 'onAnnotationDraftCreated' textlingo-desktop/src/app/routes.tsx
require_pattern 'onNavigateAnnotationSource' textlingo-desktop/src/app/routes.tsx

info "checking locator adapters"
require_pattern 'resolveAnnotation' "$LOCATOR"
require_pattern 'exact' "$LOCATOR"
require_pattern 'degraded' "$LOCATOR"
require_pattern 'unresolved' "$LOCATOR"
for kind in text_range page epub_cfi time_range; do
  require_pattern "$kind" "$LOCATOR"
done

git diff --check

if [ "$RUN_PR8" -eq 1 ]; then
  info "running PR-8 and full PR-7 regression"
  bash script/verify_pr8_data_integrity.sh --run-pr7
  info "running PR-9 annotation Playwright flow"
  (
    cd textlingo-desktop
    CI=1 npm exec -- playwright test e2e/annotation-workbench-flow.spec.ts --config playwright.config.ts
  )
fi

info "PR-9 annotation guardrails passed"
