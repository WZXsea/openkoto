#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

package_version="$(node -p 'require("./textlingo-desktop/package.json").version')"
tauri_version="$(node -p 'require("./textlingo-desktop/src-tauri/tauri.conf.json").version')"
desktop_cargo_version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' textlingo-desktop/src-tauri/Cargo.toml | head -1)"
backend_cargo_version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' openkoto-backend/Cargo.toml | head -1)"

for candidate in "$tauri_version" "$desktop_cargo_version" "$backend_cargo_version"; do
  if [ "$candidate" != "$package_version" ]; then
    echo "[release][error] version mismatch: package=$package_version tauri=$tauri_version desktop=$desktop_cargo_version backend=$backend_cargo_version" >&2
    exit 1
  fi
done

if [ -n "${EXPECTED_VERSION:-}" ] && [ "$package_version" != "$EXPECTED_VERSION" ]; then
  echo "[release][error] expected version $EXPECTED_VERSION, found $package_version" >&2
  exit 1
fi

if [ -n "${OPENKOTO_RELEASE_TAG:-}" ]; then
  case "$OPENKOTO_RELEASE_TAG" in
    v*) tag_version="${OPENKOTO_RELEASE_TAG#v}" ;;
    dev-v*) tag_version="${OPENKOTO_RELEASE_TAG#dev-v}" ;;
    wzx-v*-*)
      tag_version="${OPENKOTO_RELEASE_TAG#wzx-v}"
      tag_version="${tag_version%%-*}"
      ;;
    *)
      echo "[release][error] unsupported release tag format: $OPENKOTO_RELEASE_TAG" >&2
      exit 1
      ;;
  esac
  if [ "$tag_version" != "$package_version" ]; then
    echo "[release][error] tag $OPENKOTO_RELEASE_TAG does not match desktop version $package_version" >&2
    exit 1
  fi
fi

echo "[release] OpenKoto version $package_version is consistent"
