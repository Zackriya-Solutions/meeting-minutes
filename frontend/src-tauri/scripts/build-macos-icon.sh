#!/usr/bin/env bash

# Recompiles the native macOS app icon from the Icon Composer source package.
#
# Best-effort by design. `icons/macos-native/Assets.car` and `MementoMac.icns` are
# committed, and `tauri.macos.conf.json` bundles those committed files — so this hook
# only refreshes them when the toolchain can. Compiling an `.icon` package needs a
# newer actool than Xcode 26.6 ships (it throws on the package and exits non-zero),
# and this runs as `beforeBundleCommand`, so a hard failure here would break every
# macOS build on a stable Xcode, including CI. Warn and keep the committed artwork
# instead; regenerate on a machine with the newer tooling and commit the result.
#
# Set MEMENTO_REQUIRE_NATIVE_ICON=1 to make a failed compile fatal.

set -euo pipefail

if [[ "${TAURI_ENV_PLATFORM:-$(uname -s | tr '[:upper:]' '[:lower:]')}" != "darwin" && "${TAURI_ENV_PLATFORM:-}" != "macos" ]]; then
  exit 0
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tauri_dir="$(cd "$script_dir/.." && pwd)"
icon_package="$tauri_dir/icons/macos-source/build/MementoMac.icon"
native_output="$tauri_dir/icons/macos-native"
temporary_output="$(mktemp -d "${TMPDIR:-/tmp}/memento-native-icon.XXXXXX")"

cleanup() {
  rm -rf "$temporary_output"
}
trap cleanup EXIT

require_icon="${MEMENTO_REQUIRE_NATIVE_ICON:-0}"

# Keeps the committed icon and lets the bundle continue, unless the caller asked for
# a strict build.
skip() {
  echo "warning: $1" >&2
  if [[ "$require_icon" == "1" ]]; then
    echo "MEMENTO_REQUIRE_NATIVE_ICON=1 is set, so this is fatal." >&2
    exit 1
  fi
  echo "Keeping the committed icon at $native_output." >&2
  exit 0
}

if ! command -v xcrun >/dev/null 2>&1; then
  skip "xcrun is unavailable, so the native macOS icon cannot be recompiled."
fi

if [[ ! -d "$icon_package" ]]; then
  skip "Icon source package is missing: $icon_package"
fi

mkdir -p "$temporary_output/output" "$native_output"

# actool reports package-format errors as a plist on stdout *and* a non-zero exit, so
# capture the output to keep the bundle log readable and surface it only on failure.
if ! xcrun actool \
  --compile "$temporary_output/output" \
  --platform macosx \
  --minimum-deployment-target 13.0 \
  --target-device mac \
  --app-icon MementoMac \
  --output-partial-info-plist "$temporary_output/partial.plist" \
  "$icon_package" > "$temporary_output/actool.log" 2>&1
then
  echo "--- actool output ---" >&2
  tail -n 20 "$temporary_output/actool.log" >&2
  skip "actool could not compile $icon_package (Xcode 26.6 and older cannot read .icon packages)."
fi

# A zero exit with missing outputs would otherwise `cp` its way to a hard failure.
for artifact in Assets.car MementoMac.icns; do
  if [[ ! -f "$temporary_output/output/$artifact" ]]; then
    skip "actool succeeded but produced no $artifact."
  fi
done

cp "$temporary_output/output/Assets.car" "$native_output/Assets.car"
cp "$temporary_output/output/MementoMac.icns" "$native_output/MementoMac.icns"
cp "$temporary_output/output/MementoMac.icns" "$tauri_dir/icons/app_icon.icns"

echo "Compiled native macOS icon: $native_output/Assets.car"
