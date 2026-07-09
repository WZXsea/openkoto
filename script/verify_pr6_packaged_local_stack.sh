#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: bash script/verify_pr6_packaged_local_stack.sh [--skip-sidecar-build] [--skip-postgres-runtime] [--skip-postgres-smoke] [--skip-node-runtime] [--build-dmg]

Verifies the PR-6 packaged local stack foundation.

Default checks:
  - PR-6 design document exists.
  - Backend bundle build script exists.
  - PostgreSQL runtime bundle script exists.
  - Node runtime bundle script exists.
  - Packaged Tauri script builds the openkoto-backend resource binary.
  - Packaged Tauri script builds the PostgreSQL runtime resource.
  - Builds backend resource binary for the host Rust target.
  - Runs backend cargo check.
  - Runs Desktop cargo check/test.
  - Runs Desktop npm typecheck.

Options:
  --skip-sidecar-build  Skip building the backend sidecar binary.
  --skip-postgres-runtime
                        Skip building the bundled PostgreSQL runtime.
  --skip-postgres-smoke
                        Skip temporary initdb/postgres/createdb smoke test.
  --skip-node-runtime   Skip building the bundled Node runtime.
  --build-dmg           Also run npm --prefix textlingo-desktop run tauri:build:packaged.
USAGE
}

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

SKIP_SIDECAR_BUILD=0
SKIP_POSTGRES_RUNTIME=0
SKIP_POSTGRES_SMOKE=0
SKIP_NODE_RUNTIME=0
BUILD_DMG=0

for arg in "$@"; do
  case "$arg" in
    --skip-sidecar-build)
      SKIP_SIDECAR_BUILD=1
      ;;
    --skip-postgres-runtime)
      SKIP_POSTGRES_RUNTIME=1
      ;;
    --skip-postgres-smoke)
      SKIP_POSTGRES_SMOKE=1
      ;;
    --skip-node-runtime)
      SKIP_NODE_RUNTIME=1
      ;;
    --build-dmg)
      BUILD_DMG=1
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "[pr6][error] Unknown argument: $arg" >&2
      usage >&2
      exit 2
      ;;
  esac
done

info() {
  printf '[pr6] %s\n' "$1"
}

fail() {
  printf '[pr6][error] %s\n' "$1" >&2
  exit 1
}

postgres_env_prefix() {
  local pgroot="$1"
  export PATH="$pgroot/bin:$PATH"
  export DYLD_LIBRARY_PATH="$pgroot/lib:$pgroot/lib/postgresql:${DYLD_LIBRARY_PATH:-}"
  export LD_LIBRARY_PATH="$pgroot/lib:$pgroot/lib/postgresql:${LD_LIBRARY_PATH:-}"
}

smoke_postgres_runtime() {
  local pgroot="$1"
  local tmp_dir
  local port
  local pg_pid=""
  local initdb_log
  local postgres_log
  local createdb_log

  tmp_dir="$(mktemp -d /tmp/openkoto-pr6-postgres.XXXXXX)"
  initdb_log="$tmp_dir/initdb.log"
  postgres_log="$tmp_dir/postgres.log"
  createdb_log="$tmp_dir/createdb.log"
  port="$(node -e "const net=require('net'); const s=net.createServer(); s.listen(0,'127.0.0.1',()=>{console.log(s.address().port); s.close();});")"

  cleanup_postgres_smoke() {
    if [ -n "$pg_pid" ]; then
      kill "$pg_pid" >/dev/null 2>&1 || true
      wait "$pg_pid" >/dev/null 2>&1 || true
    fi
    rm -rf "$tmp_dir"
  }

  postgres_env_prefix "$pgroot"
  mkdir -p "$tmp_dir/socket"

  if ! "$pgroot/bin/initdb" -D "$tmp_dir/data" --username openkoto --auth-local=trust --auth-host=trust >"$initdb_log" 2>&1; then
    cat "$initdb_log" >&2 || true
    cleanup_postgres_smoke
    fail "Bundled PostgreSQL initdb smoke failed"
  fi

  "$pgroot/bin/postgres" \
    -D "$tmp_dir/data" \
    -h 127.0.0.1 \
    -p "$port" \
    -k "$tmp_dir/socket" \
    -c listen_addresses=127.0.0.1 \
    -c shared_buffers=32MB \
    -c max_connections=20 \
    -c logging_collector=off \
    >"$postgres_log" 2>&1 &
  pg_pid="$!"

  for _ in $(seq 1 80); do
    if "$pgroot/bin/createdb" -h 127.0.0.1 -p "$port" -U openkoto openkoto >"$createdb_log" 2>&1; then
      cleanup_postgres_smoke
      info "Bundled PostgreSQL smoke passed"
      return
    fi
    if grep -q "already exists" "$createdb_log" 2>/dev/null; then
      cleanup_postgres_smoke
      info "Bundled PostgreSQL smoke passed"
      return
    fi
    sleep 0.25
  done

  cat "$initdb_log" >&2 || true
  cat "$postgres_log" >&2 || true
  cat "$createdb_log" >&2 || true
  cleanup_postgres_smoke
  fail "Bundled PostgreSQL smoke timed out"
}

smoke_bundled_agent_worker() {
  local app_path
  app_path="$(cd "$1" && pwd)"
  local app_resources="$app_path/Contents/Resources"
  local node_path="$app_resources/node/bin/node"
  local worker_dir="$app_resources/agent-worker"
  local output_file
  local worker_pid=""

  output_file="$(mktemp /tmp/openkoto-pr6-worker.XXXXXX)"

  cleanup_worker_smoke() {
    if [ -n "$worker_pid" ]; then
      kill "$worker_pid" >/dev/null 2>&1 || true
      wait "$worker_pid" >/dev/null 2>&1 || true
    fi
    rm -f "$output_file"
  }

  (cd "$worker_dir" && "$node_path" dist/index.js >"$output_file" 2>&1) &
  worker_pid="$!"

  for _ in $(seq 1 40); do
    if grep -q "worker.ready" "$output_file"; then
      cleanup_worker_smoke
      info "Bundled agent-worker smoke passed"
      return
    fi
    if ! kill -0 "$worker_pid" >/dev/null 2>&1; then
      cat "$output_file" >&2 || true
      cleanup_worker_smoke
      fail "Bundled agent-worker exited before ready"
    fi
    sleep 0.25
  done

  cat "$output_file" >&2 || true
  cleanup_worker_smoke
  fail "Bundled agent-worker smoke timed out"
}

require_file() {
  local file="$1"
  if [ ! -f "$file" ]; then
    fail "Missing required file: $file"
  fi
  info "Found $file"
}

require_file "docs/plans/2026-07-09-pr6-packaged-local-stack-design.md"
require_file "script/build_pr6_backend_sidecar.sh"
require_file "script/build_pr6_postgres_runtime.sh"
require_file "script/build_pr6_node_runtime.sh"

if ! node -e "const p=require('./textlingo-desktop/package.json'); const s=p.scripts||{}; if(!String(s['build:backend-sidecar']||'').includes('build_pr6_backend_sidecar.sh')) process.exit(1); if(!String(s['build:postgres-runtime']||'').includes('build_pr6_postgres_runtime.sh')) process.exit(1); if(!String(s['build:node-runtime']||'').includes('build_pr6_node_runtime.sh')) process.exit(1); if(!String(s['tauri:build:packaged']||'').includes('build:backend-sidecar')) process.exit(1); if(!String(s['tauri:build:packaged']||'').includes('build:postgres-runtime')) process.exit(1); if(!String(s['tauri:build:packaged']||'').includes('build:node-runtime')) process.exit(1);"; then
  fail "textlingo-desktop/package.json must build backend, PostgreSQL, and Node resources before packaged Tauri build"
fi
info "Packaged Tauri script builds backend, PostgreSQL, and Node resources"

if ! node -e "const fs=require('fs'); const c=JSON.parse(fs.readFileSync('./textlingo-desktop/src-tauri/tauri.conf.json','utf8')); const r=(c.bundle||{}).resources||{}; if(r['resources/backend']!=='backend') process.exit(1); if(r['resources/postgres']!=='postgres') process.exit(1); if(r['resources/node']!=='node') process.exit(1);"; then
  fail "tauri.conf.json must bundle backend, PostgreSQL, and Node resources"
fi
info "Tauri bundles backend, PostgreSQL, and Node resources"

TARGET_TRIPLE="${OPENKOTO_BACKEND_SIDECAR_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
case "$TARGET_TRIPLE" in
  *windows-msvc|*windows-gnu)
    EXE_SUFFIX=".exe"
    ;;
  *)
    EXE_SUFFIX=""
    ;;
esac
SIDECAR_PATH="textlingo-desktop/src-tauri/binaries/openkoto-backend-$TARGET_TRIPLE$EXE_SUFFIX"
RESOURCE_BACKEND_PATH="textlingo-desktop/src-tauri/resources/backend/openkoto-backend$EXE_SUFFIX"
RESOURCE_POSTGRES_DIR="textlingo-desktop/src-tauri/resources/postgres"
RESOURCE_NODE_PATH="textlingo-desktop/src-tauri/resources/node/bin/node$EXE_SUFFIX"

if [ "$SKIP_SIDECAR_BUILD" -eq 0 ]; then
  info "Building backend sidecar"
  bash script/build_pr6_backend_sidecar.sh
fi

if [ ! -x "$SIDECAR_PATH" ]; then
  fail "Backend sidecar is missing or not executable: $SIDECAR_PATH"
fi
info "Backend sidecar is executable: $SIDECAR_PATH"

if [ ! -x "$RESOURCE_BACKEND_PATH" ]; then
  fail "Backend resource binary is missing or not executable: $RESOURCE_BACKEND_PATH"
fi
info "Backend resource binary is executable: $RESOURCE_BACKEND_PATH"

if [ "$SKIP_POSTGRES_RUNTIME" -eq 0 ]; then
  info "Building bundled PostgreSQL runtime"
  bash script/build_pr6_postgres_runtime.sh
fi

for tool in postgres initdb createdb; do
  if [ ! -x "$RESOURCE_POSTGRES_DIR/bin/$tool" ]; then
    fail "Bundled PostgreSQL tool is missing or not executable: $RESOURCE_POSTGRES_DIR/bin/$tool"
  fi
done
if [ ! -f "$RESOURCE_POSTGRES_DIR/VERSION.txt" ]; then
  fail "Bundled PostgreSQL VERSION.txt is missing"
fi
info "Bundled PostgreSQL runtime is present"

if [ "$SKIP_POSTGRES_SMOKE" -eq 0 ]; then
  info "Running bundled PostgreSQL smoke"
  smoke_postgres_runtime "$ROOT_DIR/$RESOURCE_POSTGRES_DIR"
fi

if [ "$SKIP_NODE_RUNTIME" -eq 0 ]; then
  info "Building bundled Node runtime"
  bash script/build_pr6_node_runtime.sh
fi

if [ ! -x "$RESOURCE_NODE_PATH" ]; then
  fail "Bundled Node runtime is missing or not executable: $RESOURCE_NODE_PATH"
fi
"$ROOT_DIR/$RESOURCE_NODE_PATH" --version >/dev/null
info "Bundled Node runtime is executable"

info "Running backend cargo check"
cargo check --manifest-path openkoto-backend/Cargo.toml

info "Running Desktop cargo check"
cargo check --manifest-path textlingo-desktop/src-tauri/Cargo.toml

info "Running Desktop cargo test"
cargo test --manifest-path textlingo-desktop/src-tauri/Cargo.toml

info "Running Desktop npm typecheck"
npm --prefix textlingo-desktop run typecheck

if [ "$BUILD_DMG" -eq 1 ]; then
  info "Running packaged Tauri build"
  npm --prefix textlingo-desktop run tauri:build:packaged
  if ! find textlingo-desktop/src-tauri/target -path '*bundle/dmg/*.dmg' -type f | grep -q .; then
    fail "Packaged Tauri build finished but no dmg was found"
  fi
  app_path="$(find textlingo-desktop/src-tauri/target -path '*bundle/macos/*.app' -type d | head -1)"
  if [ -z "$app_path" ]; then
    fail "Packaged Tauri build finished but no app bundle was found"
  fi
  app_backend="$app_path/Contents/Resources/backend/openkoto-backend$EXE_SUFFIX"
  if [ ! -x "$app_backend" ]; then
    fail "Packaged app is missing backend resource binary: $app_backend"
  fi
  app_postgres="$app_path/Contents/Resources/postgres/bin/postgres"
  if [ ! -x "$app_postgres" ]; then
    fail "Packaged app is missing PostgreSQL runtime: $app_postgres"
  fi
  app_node="$app_path/Contents/Resources/node/bin/node$EXE_SUFFIX"
  if [ ! -x "$app_node" ]; then
    fail "Packaged app is missing Node runtime: $app_node"
  fi
  "$app_node" --version >/dev/null
  smoke_bundled_agent_worker "$app_path"
  info "Packaged dmg exists"
fi

info "PR-6 packaged local stack checks completed"
