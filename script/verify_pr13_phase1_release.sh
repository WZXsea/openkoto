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
Usage: bash script/verify_pr13_phase1_release.sh [--full] [--history] [--start-db] [--package-smoke]

PR-13 phase-one release-candidate verification.

Default checks are static and offline-safe: release version consistency, shell
syntax, formatting, packaged-runtime safety anchors, backup/restore contracts,
legacy-import reporting, and clean diffs.

Options:
  --full           Run the PR-12 full Backend/worker/Desktop/frontend/E2E suite.
  --history        Chain PR-12 through the historical PR-11 and earlier gates.
  --start-db       Start an isolated Docker PostgreSQL database for --full.
  --package-smoke  Build packaged sidecars/runtimes and a local DMG via PR-6 QA.
  -h, --help       Show this help.

Signed distribution, Developer ID signing, notarization, stapling, and
Gatekeeper verification require Apple Developer credentials and are not
performed by this script.
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --full) RUN_FULL=1 ;;
    --history) RUN_HISTORY=1 ;;
    --start-db) START_DB=1 ;;
    --package-smoke) PACKAGE_SMOKE=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "[PR-13] ERROR: unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

if [ "$START_DB" -eq 1 ] && [ "$RUN_FULL" -ne 1 ]; then
  echo "[PR-13] ERROR: --start-db requires --full" >&2
  exit 2
fi

info() {
  echo "[PR-13] $*"
}

fail() {
  echo "[PR-13] ERROR: $*" >&2
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

require_max_lines() {
  local maximum="$1"
  local file="$2"
  local actual
  actual="$(wc -l < "$file" | tr -d ' ')"
  [ "$actual" -le "$maximum" ] || fail "$file grew to $actual lines; PR-13 boundary is $maximum"
}

desktop_version() {
  awk '
    /^\[package\]$/ { in_package=1; next }
    /^\[/ && in_package { exit }
    in_package && /^version = "/ {
      value=$0
      sub(/^version = "/, "", value)
      sub(/"$/, "", value)
      print value
      exit
    }
  ' textlingo-desktop/src-tauri/Cargo.toml
}

EXPECTED_VERSION="${EXPECTED_VERSION:-$(desktop_version)}"
[ -n "$EXPECTED_VERSION" ] || fail "could not resolve release version"
export EXPECTED_VERSION

for path in \
  textlingo-desktop/src-tauri/src/packaged_backend.rs \
  textlingo-desktop/src-tauri/src/data_backup.rs \
  textlingo-desktop/src-tauri/src/legacy_import.rs \
  textlingo-desktop/src-tauri/src/app_config/commands.rs \
  textlingo-desktop/src-tauri/src/app_config/service.rs \
  textlingo-desktop/src/components/features/reader/ArticleReaderHeader.tsx \
  textlingo-desktop/src/components/features/reader/ArticleReaderAssistantShell.tsx \
  textlingo-desktop/src/components/features/settings/AppearanceSettingsPanel.tsx \
  textlingo-desktop/src/components/features/settings/LanguageSettingsPanel.tsx \
  textlingo-desktop/src/components/features/settings/AdvancedSettingsPanel.tsx \
  textlingo-desktop/src/components/features/settings/RuntimeLogsSettingsPanel.tsx \
  docs/architecture/pr13-phase1-engineering-release.md \
  docs/architecture/pr8-data-ownership-and-backup.md \
  docs/releases/pr13-phase1-release-qa.md \
  script/check_release_workflows.py \
  script/test_release_workflows.py \
  script/verify_pr12_assistant_observability.sh \
  script/verify_release_version.sh; do
  require_file "$path"
done

info "checking shell syntax, Rust formatting, version ${EXPECTED_VERSION}, and diff hygiene"
bash -n script/verify_pr13_phase1_release.sh
bash -n script/verify_pr12_assistant_observability.sh
cargo fmt --manifest-path textlingo-desktop/src-tauri/Cargo.toml --all -- --check
EXPECTED_VERSION="$EXPECTED_VERSION" bash script/verify_release_version.sh
python3 script/check_release_workflows.py
python3 -m unittest script/test_release_workflows.py
git diff --check

info "checking PR-13 module boundaries and compatibility registrations"
require_pattern 'app_config::commands::backend_check_session_cmd' textlingo-desktop/src-tauri/src/lib.rs
require_pattern 'CONFIG_AUTH_COMMAND_NAMES' textlingo-desktop/src-tauri/src/app_config/commands.rs
require_pattern 'ArticleReaderHeader' textlingo-desktop/src/components/features/ArticleReader.tsx
require_pattern 'ArticleReaderAssistantShell' textlingo-desktop/src/components/features/ArticleReader.tsx
require_pattern 'AppearanceSettingsPanel' textlingo-desktop/src/components/features/SettingsDialog.tsx
require_pattern 'RuntimeLogsSettingsPanel' textlingo-desktop/src/components/features/SettingsDialog.tsx
require_pattern 'refreshTimeline' textlingo-desktop/src/features/assistant/state.ts
require_pattern '连接诊断' textlingo-desktop/src/components/features/BackendConnectionGate.tsx
require_max_lines 6400 textlingo-desktop/src-tauri/src/commands.rs
require_max_lines 1600 textlingo-desktop/src/components/features/ArticleReader.tsx
require_max_lines 1800 textlingo-desktop/src/components/features/SettingsDialog.tsx

info "checking packaged runtime and recovery diagnostics"
for pattern in \
  'wait_for_backend_health_or_exit' \
  'upgrade_recovery_diagnostic' \
  'new backend fingerprint was not committed' \
  'inspect_postgres_data_dir' \
  'write_atomic_text' \
  'command_starts_with_executable' \
  'has_bounded_text' \
  'refusing to stop an unrelated process'; do
  require_pattern "$pattern" textlingo-desktop/src-tauri/src/packaged_backend.rs
done
python3 - <<'PY'
from pathlib import Path

source = Path("textlingo-desktop/src-tauri/src/packaged_backend.rs").read_text()
start = source.index("fn ensure_packaged_backend_port_available")
end = source.index("#[cfg(unix)]\nfn packaged_backend_port_occupant", start)
body = source[start:end]
if "terminate_pid" in body or "Command::new(\"kill\")" in body:
    raise SystemExit("port-conflict path must diagnose only and cannot terminate a process")
PY

info "checking backup manifest, checksums, copied files, and dry restore"
for pattern in \
  'BACKUP_FORMAT_VERSION: u32 = 2' \
  'manifest.sha256' \
  'snapshot_entries' \
  'validate_manifest_checksum' \
  'copy_dir_recursive\(&backend_dir.join\("files"\)' \
  'dry restore checksum verification failed'; do
  require_pattern "$pattern" textlingo-desktop/src-tauri/src/data_backup.rs
done

info "checking verifiable legacy-import source report"
for pattern in \
  'LegacyImportSourceReport' \
  'source_sha256' \
  'SOURCE_REPORT_KEY' \
  'verify_legacy_import_batch' \
  'source JSON was retained'; do
  require_pattern "$pattern" textlingo-desktop/src-tauri/src/legacy_import.rs
done

if [ "$RUN_FULL" -eq 1 ]; then
  info "running full PR-12 verification as the PR-13 functional baseline"
  pr12_args=(--full)
  [ "$START_DB" -eq 1 ] && pr12_args+=(--start-db)
  [ "$RUN_HISTORY" -eq 1 ] && pr12_args+=(--run-pr11)
  bash script/verify_pr12_assistant_observability.sh "${pr12_args[@]}"
elif [ "$RUN_HISTORY" -eq 1 ]; then
  info "running historical static/regression chain"
  bash script/verify_pr12_assistant_observability.sh --run-pr11
fi

if [ "$PACKAGE_SMOKE" -eq 1 ]; then
  info "building packaged runtimes and local DMG"
  bash script/verify_pr6_packaged_local_stack.sh --build-dmg
fi

info "PR-13 phase-one release-candidate guardrails passed"
