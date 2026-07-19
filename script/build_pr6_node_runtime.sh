#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NODE_PROGRAM="${NODE_PROGRAM:-$(command -v node || true)}"
RESOURCE_NODE_DIR="$ROOT_DIR/textlingo-desktop/src-tauri/resources/node"
TMP_DIR="$RESOURCE_NODE_DIR.tmp"

info() {
  printf '[pr6] %s\n' "$1"
}

fail() {
  printf '[pr6][error] %s\n' "$1" >&2
  exit 1
}

if [ -z "$NODE_PROGRAM" ] || [ ! -x "$NODE_PROGRAM" ]; then
  fail "Node runtime not found. Set NODE_PROGRAM to a node executable."
fi

VERSION="$("$NODE_PROGRAM" --version)"
info "Building bundled Node runtime from $NODE_PROGRAM ($VERSION)"

rm -rf "$TMP_DIR"
mkdir -p "$TMP_DIR/bin"
cp -L "$NODE_PROGRAM" "$TMP_DIR/bin/node"
chmod +x "$TMP_DIR/bin/node"

command -v xattr >/dev/null 2>&1 && xattr -cr "$TMP_DIR" >/dev/null 2>&1 || true
if command -v codesign >/dev/null 2>&1; then
  codesign --force --sign - "$TMP_DIR/bin/node" >/dev/null 2>&1 || true
fi

cat > "$TMP_DIR/VERSION.txt" <<EOF
$VERSION
source=$NODE_PROGRAM
EOF

rm -rf "$RESOURCE_NODE_DIR"
mv "$TMP_DIR" "$RESOURCE_NODE_DIR"
touch "$RESOURCE_NODE_DIR/.gitkeep"
info "Node runtime copied to $RESOURCE_NODE_DIR"
