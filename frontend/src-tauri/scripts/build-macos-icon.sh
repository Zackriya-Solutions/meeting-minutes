#!/usr/bin/env bash

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

if ! command -v xcrun >/dev/null 2>&1; then
  echo "Xcode 26 or newer is required to compile the native Memento icon." >&2
  exit 1
fi

mkdir -p "$temporary_output/output" "$native_output"

xcrun actool \
  --compile "$temporary_output/output" \
  --platform macosx \
  --minimum-deployment-target 13.0 \
  --target-device mac \
  --app-icon MementoMac \
  --output-partial-info-plist "$temporary_output/partial.plist" \
  "$icon_package"

cp "$temporary_output/output/Assets.car" "$native_output/Assets.car"
cp "$temporary_output/output/MementoMac.icns" "$native_output/MementoMac.icns"
cp "$temporary_output/output/MementoMac.icns" "$tauri_dir/icons/app_icon.icns"

echo "Compiled native macOS icon: $native_output/Assets.car"
