#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RUN_FULL=0
RUN_HISTORY=0
START_DB=0
PACKAGE_SMOKE=0

usage() {
  cat <<'USAGE'
Usage: bash script/verify_pr14_structured_editor.sh [--full] [--history] [--start-db] [--package-smoke]

PR-14 structured document editor verification.

Options:
  --full           Run Backend, Desktop, frontend build/test and editor E2E gates.
  --history        Run the PR-13 and earlier historical verification chain.
  --start-db       Start the isolated PostgreSQL database used by --full.
  --package-smoke  Build packaged runtimes and a local DMG.
  -h, --help       Show this help.
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --full) RUN_FULL=1 ;;
    --history) RUN_HISTORY=1 ;;
    --start-db) START_DB=1 ;;
    --package-smoke) PACKAGE_SMOKE=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "[PR-14] ERROR: unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

if [ "$START_DB" -eq 1 ] && [ "$RUN_FULL" -ne 1 ]; then
  echo "[PR-14] ERROR: --start-db requires --full" >&2
  exit 2
fi

info() {
  echo "[PR-14] $*"
}

fail() {
  echo "[PR-14] ERROR: $*" >&2
  exit 1
}

finalize_local_macos_package() {
  [ "$(uname -s)" = "Darwin" ] || fail "local DMG finalization requires macOS"

  local app_path
  local dmg_path
  local dmg_script
  local macos_dir
  local mount_dir
  local mounted_app
  local bundle_version

  app_path="$(find textlingo-desktop/src-tauri/target -path '*bundle/macos/*.app' -type d | head -1)"
  dmg_path="$(find textlingo-desktop/src-tauri/target -path '*bundle/dmg/*.dmg' -type f | head -1)"
  [ -n "$app_path" ] || fail "packaged app bundle was not found"
  [ -n "$dmg_path" ] || fail "packaged DMG was not found"

  dmg_script="$(dirname "$dmg_path")/bundle_dmg.sh"
  macos_dir="$(dirname "$app_path")"
  [ -x "$dmg_script" ] || fail "generated DMG bundler was not found: $dmg_script"

  info "applying local deep ad-hoc signature"
  codesign --force --deep --sign - --options runtime --timestamp=none "$app_path"
  codesign --verify --deep --strict --verbose=2 "$app_path"

  info "rebuilding DMG from the signed app bundle"
  rm -f "$dmg_path"
  "$dmg_script" \
    --volname 'OpenKoto Desktop' \
    --app-drop-link 380 205 \
    --icon 'OpenKoto Desktop.app' 120 205 \
    --hide-extension 'OpenKoto Desktop.app' \
    "$dmg_path" \
    "$macos_dir"
  hdiutil verify "$dmg_path" >/dev/null

  mount_dir="$(mktemp -d /tmp/openkoto-pr14-dmg.XXXXXX)"
  if ! hdiutil attach -readonly -nobrowse -mountpoint "$mount_dir" "$dmg_path" >/dev/null; then
    rm -rf "$mount_dir"
    fail "signed DMG could not be mounted"
  fi
  mounted_app="$(find "$mount_dir" -maxdepth 1 -name '*.app' -type d | head -1)"
  if [ -z "$mounted_app" ] || ! codesign --verify --deep --strict --verbose=2 "$mounted_app"; then
    hdiutil detach "$mount_dir" >/dev/null 2>&1 || true
    rm -rf "$mount_dir"
    fail "mounted app failed deep signature verification"
  fi
  bundle_version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$mounted_app/Contents/Info.plist")"
  hdiutil detach "$mount_dir" >/dev/null
  rm -rf "$mount_dir"
  [ "$bundle_version" = "0.12.0" ] || fail "unexpected packaged app version: $bundle_version"

  info "signed DMG verified: $(shasum -a 256 "$dmg_path" | awk '{print $1}')"
}

require_file() {
  [ -f "$1" ] || fail "missing required file: $1"
}

require_pattern() {
  local pattern="$1"
  local file="$2"
  rg -q -- "$pattern" "$file" || fail "missing pattern '$pattern' in $file"
}

for path in \
  openkoto-backend/migrations/20260715001300_document_editing.sql \
  openkoto-backend/migrations/20260715001400_document_lineage.sql \
  openkoto-backend/migrations/20260715001500_document_migration_reports.sql \
  openkoto-backend/src/document_editing.rs \
  openkoto-backend/tests/document_editing.rs \
  textlingo-desktop/src-tauri/src/document_editor.rs \
  textlingo-desktop/src/features/editor/MaterialDocumentEditor.tsx \
  textlingo-desktop/src/features/editor/materialEditorApi.ts \
  textlingo-desktop/e2e/material-editor-flow.spec.ts \
  docs/architecture/pr14-structured-document-editing.md; do
  require_file "$path"
done

for pattern in \
  'material_blocks' \
  'material_revisions' \
  'material_edit_drafts'; do
  require_pattern "$pattern" openkoto-backend/migrations/20260715001300_document_editing.sql
done

for pattern in \
  'material_segment_lineage' \
  'material_relations' \
  "'divider'"; do
  require_pattern "$pattern" openkoto-backend/migrations/20260715001400_document_lineage.sql
done

require_pattern 'material_document_migration_reports' openkoto-backend/migrations/20260715001500_document_migration_reports.sql

for pattern in \
  'material_revision_conflict' \
  'document_edit_idempotency_conflict' \
  'document_preview_changed' \
  'preview_token' \
  'preview_document_edit' \
  'commit_document_edit' \
  'restore_revision' \
  'patch_segment_derived'; do
  require_pattern "$pattern" openkoto-backend/src/document_editing.rs
done

for command in \
  get_material_document_cmd \
  preview_material_edit_cmd \
  commit_material_edit_cmd \
  get_material_draft_cmd \
  save_material_draft_cmd \
  delete_material_draft_cmd \
  list_material_revisions_cmd \
  get_material_revision_cmd \
  restore_material_revision_cmd \
  update_segment_derived_cmd \
  create_editable_derivative_cmd; do
  require_pattern "$command" textlingo-desktop/src-tauri/src/document_editor.rs
  require_pattern "document_editor::$command" textlingo-desktop/src-tauri/src/lib.rs
done

require_pattern 'material_revision_conflict' textlingo-desktop/src/features/editor/materialEditorApi.ts
require_pattern 'preview_token' textlingo-desktop/src/features/editor/MaterialDocumentEditor.tsx
require_pattern 'MaterialDocumentEditor' textlingo-desktop/src/components/features/ArticleReader.tsx
require_pattern 'replace_backend_media_subtitles_legacy' textlingo-desktop/src-tauri/src/commands.rs
if rg -q 'replace_backend_article' textlingo-desktop/src-tauri/src; then
  fail "unrestricted replace_backend_article path remains"
fi

bash -n script/verify_pr14_structured_editor.sh
info "running PR-13 structural and release-version guardrails"
bash script/verify_pr13_phase1_release.sh

if [ "$RUN_FULL" -eq 1 ]; then
  info "running PR-14 full verification"
  pr13_args=(--full)
  [ "$START_DB" -eq 1 ] && pr13_args+=(--start-db)
  [ "$RUN_HISTORY" -eq 1 ] && pr13_args+=(--history)
  bash script/verify_pr13_phase1_release.sh "${pr13_args[@]}"
  if [ "$RUN_HISTORY" -eq 1 ]; then
    info "document editing tests were covered by the historical full Backend runs"
  else
    cargo test --manifest-path openkoto-backend/Cargo.toml --test document_editing
  fi
  cargo test --manifest-path textlingo-desktop/src-tauri/Cargo.toml --lib document_editor
  npm --prefix textlingo-desktop run typecheck
  npm --prefix textlingo-desktop test -- --run
  npm --prefix textlingo-desktop run build:all
  npm --prefix textlingo-desktop run e2e -- material-editor-flow.spec.ts
elif [ "$RUN_HISTORY" -eq 1 ]; then
  info "running historical regression chain"
  bash script/verify_pr13_phase1_release.sh --history
fi

if [ "$PACKAGE_SMOKE" -eq 1 ]; then
  info "building packaged runtimes and local DMG"
  bash script/verify_pr13_phase1_release.sh --package-smoke
  finalize_local_macos_package
fi

info "PR-14 structured editor guardrails passed"
