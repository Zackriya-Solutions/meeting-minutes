#!/usr/bin/env bash
# VALUEOS: stage the ValueOS Agent icons into the Tauri icon directory at BUILD TIME.
#
# Source of truth: valueos/branding/icons/  (our parallel folder — never edited by upstream)
# Target:          frontend/src-tauri/icons/ (upstream folder — overwritten only in the
#                  working tree, never committed; a fresh checkout/CI run is unaffected)
#
# Why copy instead of pointing tauri.conf.json at our folder: Tauri resolves bundle.icon
# paths relative to tauri.conf.json's own directory, so reusing the existing filenames is
# the robust, version-proof way to swap icons without editing any upstream file.
#
# Run from anywhere. Pair it with:  tauri build --config valueos/branding/tauri.valueos.json
# (adjust the --config path to be relative to your cwd; from `frontend/` it is
#  ../valueos/branding/tauri.valueos.json)
#
# Local note: this leaves frontend/src-tauri/icons/ modified in your working tree. To
# restore upstream icons afterwards:  git checkout -- frontend/src-tauri/icons
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SRC="$ROOT/valueos/branding/icons"
DST="$ROOT/frontend/src-tauri/icons"

if [ ! -d "$DST" ]; then
  echo "error: $DST not found — run from within the repo" >&2
  exit 1
fi

# Only the files tauri.conf.json's bundle.icon references, plus common aliases.
for f in icon.png app_icon.icns app_icon.ico icon.icns icon.ico 32x32.png 128x128.png 128x128@2x.png; do
  if [ -f "$SRC/$f" ]; then
    cp "$SRC/$f" "$DST/$f"
    echo "  staged $f"
  fi
done

echo "ValueOS Agent icons staged into $DST"
