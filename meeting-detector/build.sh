#!/usr/bin/env bash
#
# Build MementoDetector.app — a tiny menu-bar auto-recorder bundle.
#
# Produces meeting-detector/MementoDetector.app with the release binary, the
# ffmpeg sidecar (copied from the main app), an icon, and Info.plist, then
# ad-hoc code-signs it so macOS mic permission (TCC) and notifications work.
#
# For distribution, re-sign the bundle with your Developer ID + hardened runtime
# and macos/entitlements.plist (see the main app's build-mac-signed.sh).

set -euo pipefail

cd "$(dirname "$0")"                       # meeting-detector/
ROOT="$(cd .. && pwd)"                      # repo root
ARCH="$(uname -m | sed 's/arm64/aarch64/')" # aarch64 | x86_64
APP="MementoDetector.app"
FFMPEG_SIDECAR="$ROOT/frontend/src-tauri/binaries/ffmpeg-${ARCH}-apple-darwin"

echo "==> Building release binary (${ARCH}) ..."
cargo build --release -p meeting-detector

if [[ ! -x "$FFMPEG_SIDECAR" ]]; then
    echo "!! ffmpeg sidecar not found at: $FFMPEG_SIDECAR" >&2
    echo "   Build the main app once so the sidecar is present, or set MEMENTO_DETECTOR_FFMPEG." >&2
    exit 1
fi

echo "==> Assembling ${APP} ..."
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ROOT/target/release/memento-detector" "$APP/Contents/MacOS/memento-detector"
cp "$FFMPEG_SIDECAR" "$APP/Contents/MacOS/ffmpeg"
chmod +x "$APP/Contents/MacOS/ffmpeg"
cp "macos/Info.plist" "$APP/Contents/Info.plist"
cp "icons/app.icns" "$APP/Contents/Resources/app.icns"

echo "==> Ad-hoc code-signing ..."
codesign --force --deep --sign - "$APP"

echo
echo "Done: $(pwd)/$APP"
echo
echo "Launch it:            open '$(pwd)/$APP'"
echo "Run with logs:        RUST_LOG=debug '$(pwd)/$APP/Contents/MacOS/memento-detector'"
echo "Grant mic access on first recording (System Settings → Privacy & Security → Microphone)."
