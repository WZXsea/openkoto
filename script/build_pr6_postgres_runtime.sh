#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
POSTGRES_CONFIG="${POSTGRES_CONFIG:-pg_config}"
RESOURCE_POSTGRES_DIR="$ROOT_DIR/textlingo-desktop/src-tauri/resources/postgres"
TMP_DIR="$RESOURCE_POSTGRES_DIR.tmp"

info() {
  printf '[pr6] %s\n' "$1"
}

fail() {
  printf '[pr6][error] %s\n' "$1" >&2
  exit 1
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    fail "Required command not found: $1"
  fi
}

copy_tree() {
  local source="$1"
  local dest="$2"
  mkdir -p "$dest"
  cp -R -L "$source/." "$dest/"
}

is_macho() {
  file "$1" 2>/dev/null | grep -q 'Mach-O'
}

copy_dylib_dependency() {
  local dep="$1"
  local dep_name
  local source_path=""
  dep_name="$(basename "$dep")"

  case "$dep" in
    /opt/homebrew/*.dylib|/opt/homebrew/*/*.dylib|/opt/homebrew/*/*/*.dylib|/opt/homebrew/*/*/*/*.dylib)
      source_path="$dep"
      ;;
    @loader_path/*.dylib|@rpath/*.dylib)
      source_path="$(find /opt/homebrew/opt /opt/homebrew/Cellar -name "$dep_name" 2>/dev/null | head -1 || true)"
      ;;
    *)
      return 1
      ;;
  esac

  if [ -n "$source_path" ] && [ -f "$source_path" ] && [ ! -f "$TMP_DIR/lib/$dep_name" ]; then
    cp -L "$source_path" "$TMP_DIR/lib/$dep_name"
    chmod u+w "$TMP_DIR/lib/$dep_name" || true
    return 0
  fi

  return 1
}

copy_homebrew_dylib_deps() {
  local changed=1
  while [ "$changed" -eq 1 ]; do
    changed=0
    while IFS= read -r macho_file; do
      is_macho "$macho_file" || continue
      while IFS= read -r dep; do
        if copy_dylib_dependency "$dep"; then
          changed=1
        fi
      done < <(otool -L "$macho_file" 2>/dev/null | awk 'NR > 1 { print $1 }')
    done < <(find "$TMP_DIR/bin" "$TMP_DIR/lib" -type f)
  done
}

rewrite_macho_library_paths() {
  command -v install_name_tool >/dev/null 2>&1 || return 0

  while IFS= read -r dylib_file; do
    is_macho "$dylib_file" || continue
    case "$(basename "$dylib_file")" in
      *.dylib)
        install_name_tool -id "@rpath/$(basename "$dylib_file")" "$dylib_file" 2>/dev/null || true
        ;;
    esac
  done < <(find "$TMP_DIR/lib" -type f)

  while IFS= read -r macho_file; do
    is_macho "$macho_file" || continue
    install_name_tool -add_rpath '@executable_path/../lib' "$macho_file" 2>/dev/null || true
    install_name_tool -add_rpath '@loader_path' "$macho_file" 2>/dev/null || true
    install_name_tool -add_rpath '@loader_path/..' "$macho_file" 2>/dev/null || true
    while IFS= read -r dep; do
      case "$dep" in
        /opt/homebrew/*.dylib|/opt/homebrew/*/*.dylib|/opt/homebrew/*/*/*.dylib|/opt/homebrew/*/*/*/*.dylib)
          install_name_tool -change "$dep" "@rpath/$(basename "$dep")" "$macho_file" 2>/dev/null || true
          ;;
      esac
    done < <(otool -L "$macho_file" 2>/dev/null | awk 'NR > 1 { print $1 }')
  done < <(find "$TMP_DIR/bin" "$TMP_DIR/lib" -type f)
}

codesign_macho_files() {
  command -v codesign >/dev/null 2>&1 || return 0

  while IFS= read -r macho_file; do
    is_macho "$macho_file" || continue
    chmod u+w "$macho_file" || true
    codesign --force --sign - "$macho_file" >/dev/null 2>&1 || true
  done < <(find "$TMP_DIR/bin" "$TMP_DIR/lib" -type f)
}

prune_development_files() {
  find "$TMP_DIR/lib" -name '*.a' -delete
  rm -rf "$TMP_DIR/lib/pkgconfig"
  rm -rf "$TMP_DIR/lib/postgresql/pgxs"
}

clear_extended_attributes() {
  command -v xattr >/dev/null 2>&1 || return 0
  xattr -cr "$TMP_DIR" >/dev/null 2>&1 || true
}

normalize_runtime_permissions() {
  chmod -R u+rwX,go+rX "$TMP_DIR"
  find "$TMP_DIR/bin" -type f -exec chmod +x {} \;
}

require_command "$POSTGRES_CONFIG"
require_command otool
require_command file

BINDIR="$("$POSTGRES_CONFIG" --bindir)"
LIBDIR="$("$POSTGRES_CONFIG" --libdir)"
SHAREDIR="$("$POSTGRES_CONFIG" --sharedir)"
PKGLIBDIR="$("$POSTGRES_CONFIG" --pkglibdir)"
VERSION="$("$POSTGRES_CONFIG" --version)"
SHARE_BASENAME="$(basename "$SHAREDIR")"

for tool in postgres initdb createdb pg_ctl; do
  if [ ! -x "$BINDIR/$tool" ]; then
    fail "Required PostgreSQL tool is missing: $BINDIR/$tool"
  fi
done

info "Building bundled PostgreSQL runtime from $VERSION"
rm -rf "$TMP_DIR"
mkdir -p "$TMP_DIR/bin" "$TMP_DIR/lib" "$TMP_DIR/share"

for tool in postgres initdb createdb pg_ctl; do
  cp -L "$BINDIR/$tool" "$TMP_DIR/bin/$tool"
  chmod +x "$TMP_DIR/bin/$tool"
done

copy_tree "$LIBDIR" "$TMP_DIR/lib"
if [ "$PKGLIBDIR" != "$LIBDIR/postgresql" ]; then
  copy_tree "$PKGLIBDIR" "$TMP_DIR/lib/postgresql"
fi
copy_tree "$SHAREDIR" "$TMP_DIR/share/$SHARE_BASENAME"
copy_homebrew_dylib_deps
rewrite_macho_library_paths
prune_development_files
clear_extended_attributes
normalize_runtime_permissions
codesign_macho_files

cat > "$TMP_DIR/VERSION.txt" <<EOF
$VERSION
source_bindir=$BINDIR
source_libdir=$LIBDIR
source_sharedir=$SHAREDIR
source_pkglibdir=$PKGLIBDIR
EOF

rm -rf "$RESOURCE_POSTGRES_DIR"
mv "$TMP_DIR" "$RESOURCE_POSTGRES_DIR"
touch "$RESOURCE_POSTGRES_DIR/.gitkeep"
info "PostgreSQL runtime copied to $RESOURCE_POSTGRES_DIR"
