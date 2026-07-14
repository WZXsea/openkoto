#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RUN_FULL=0
RUN_PR10=0

usage() {
  cat <<'USAGE'
Usage: bash script/verify_pr11_reading_home.sh [--full] [--run-pr10]

Checks the PR-11 reading home, material-library split, activity heatmap,
source-aware navigation, Desktop bridge, and 0.9.0 release contracts.

Options:
  --full      Run Backend, Desktop, frontend, build, and Playwright verification.
  --run-pr10  Chain the PR-10 and earlier static/regression guardrails.
  -h, --help  Show this help.
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --full) RUN_FULL=1 ;;
    --run-pr10) RUN_PR10=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "[PR-11] ERROR: unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

info() {
  echo "[PR-11] $*"
}

fail() {
  echo "[PR-11] ERROR: $*" >&2
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

HOME_PAGE="textlingo-desktop/src/components/features/HomePage.tsx"
MATERIALS_PAGE="textlingo-desktop/src/components/features/MaterialsWorkbenchPage.tsx"
HOME_API="textlingo-desktop/src/features/home/api.ts"
ACTIVITY="openkoto-backend/src/learning_activity.rs"
BACKEND_TEST="openkoto-backend/tests/pr11_activity_heatmap.rs"

for path in \
  "$HOME_PAGE" \
  "$MATERIALS_PAGE" \
  "$HOME_API" \
  "$ACTIVITY" \
  "$BACKEND_TEST" \
  docs/architecture/pr11-reading-home-and-app-shell.md \
  textlingo-desktop/src-tauri/src/backend_client.rs \
  textlingo-desktop/src-tauri/src/commands.rs \
  textlingo-desktop/src/app/AppShell.tsx \
  textlingo-desktop/src/app/appStore.ts \
  textlingo-desktop/src/app/navigation.ts \
  textlingo-desktop/src/app/routes.tsx \
  textlingo-desktop/src/components/features/HomePage.test.tsx \
  textlingo-desktop/src/components/features/MaterialsWorkbenchPage.test.tsx \
  textlingo-desktop/e2e/material-workbench-flow.spec.ts; do
  require_file "$path"
done

info "checking shell syntax, formatting, locales, and version"
bash -n script/verify_pr11_reading_home.sh
git diff --check
node -e 'for (const file of ["en", "ja", "zh"]) JSON.parse(require("fs").readFileSync(`textlingo-desktop/src/locales/${file}.json`, "utf8"))'
EXPECTED_VERSION=0.9.0 bash script/verify_release_version.sh

info "checking activity heatmap contract"
for pattern in \
  'DEFAULT_ACTIVITY_HEATMAP_DAYS: i64 = 84' \
  'MAX_ACTIVITY_HEATMAP_DAYS: i64 = 366' \
  'get_activity_heatmap' \
  'COUNT\(DISTINCT material_id\)' \
  "'local_preview', 'merge'" \
  "AT TIME ZONE 'UTC'"; do
  require_pattern "$pattern" "$ACTIVITY"
done
require_pattern '/learning-review/activity-heatmap' openkoto-backend/src/routes.rs
require_pattern 'get_learning_activity_heatmap' textlingo-desktop/src-tauri/src/backend_client.rs
require_pattern 'get_learning_activity_heatmap_cmd' textlingo-desktop/src-tauri/src/commands.rs
require_pattern 'get_learning_activity_heatmap_cmd' textlingo-desktop/src-tauri/src/lib.rs

info "checking home, material library, and navigation contracts"
for pattern in 'id: "home"' 'id: "materials"' 'fallbackLabel: "首页"' 'fallbackLabel: "素材库"'; do
  require_pattern "$pattern" textlingo-desktop/src/app/navigation.ts
done
for pattern in 'openMaterials' 'materialsScrollTop' 'materialFilters' 'ReaderReturnScreen'; do
  require_pattern "$pattern" textlingo-desktop/src/app/appStore.ts
done
for pattern in '近12周' '今日摘要' '活动摘要暂时无法读取，不影响阅读' 'onOpenMaterials'; do
  require_pattern "$pattern" "$HOME_PAGE"
done
for pattern in '素材库页签' 'MaterialTagsPanel' 'MaterialImportJobsPanel' 'showContinueReading=\{false\}'; do
  require_pattern "$pattern" "$MATERIALS_PAGE"
done
require_pattern 'aria-label="主导航"' textlingo-desktop/src/app/AppShell.tsx
require_pattern '!store.selectedArticle' textlingo-desktop/src/app/AppShell.tsx

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

  info "running Playwright workflows"
  (
    cd textlingo-desktop
    CI=1 npm exec -- playwright test --config playwright.config.ts
  )
fi

if [ "$RUN_PR10" -eq 1 ]; then
  info "running PR-10 and earlier regression guardrails"
  bash script/verify_pr10_learning_domain.sh --run-pr9
fi

info "PR-11 reading-home guardrails passed"
