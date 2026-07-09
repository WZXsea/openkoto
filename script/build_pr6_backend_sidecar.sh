#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_TRIPLE="${OPENKOTO_BACKEND_SIDECAR_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
TARGET_DIR="$ROOT_DIR/textlingo-desktop/src-tauri/binaries"
RESOURCE_BACKEND_DIR="$ROOT_DIR/textlingo-desktop/src-tauri/resources/backend"

if [ -z "$TARGET_TRIPLE" ]; then
  echo "[pr6][error] Could not determine Rust target triple" >&2
  exit 1
fi

case "$TARGET_TRIPLE" in
  *windows-msvc|*windows-gnu)
    EXE_SUFFIX=".exe"
    ;;
  *)
    EXE_SUFFIX=""
    ;;
esac

echo "[pr6] Building openkoto-backend sidecar for $TARGET_TRIPLE"
cargo build \
  --manifest-path "$ROOT_DIR/openkoto-backend/Cargo.toml" \
  --release \
  --target "$TARGET_TRIPLE"

SOURCE_BINARY="$ROOT_DIR/openkoto-backend/target/$TARGET_TRIPLE/release/openkoto-backend$EXE_SUFFIX"
DEST_BINARY="$TARGET_DIR/openkoto-backend-$TARGET_TRIPLE$EXE_SUFFIX"
RESOURCE_BINARY="$RESOURCE_BACKEND_DIR/openkoto-backend$EXE_SUFFIX"

if [ ! -f "$SOURCE_BINARY" ]; then
  echo "[pr6][error] Backend binary was not produced: $SOURCE_BINARY" >&2
  exit 1
fi

mkdir -p "$TARGET_DIR"
mkdir -p "$RESOURCE_BACKEND_DIR"
cp "$SOURCE_BINARY" "$DEST_BINARY"
cp "$SOURCE_BINARY" "$RESOURCE_BINARY"
chmod +x "$DEST_BINARY"
chmod +x "$RESOURCE_BINARY"

echo "[pr6] Backend sidecar copied to $DEST_BINARY"
echo "[pr6] Backend resource binary copied to $RESOURCE_BINARY"
