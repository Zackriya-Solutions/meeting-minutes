#!/usr/bin/env bash
# One-shot build for the Meetily personal fork (local-only).
# Verifies prerequisites, prepares the sidecar + models, then bundles the .dmg.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"

say() { printf "\n\033[1;36m==> %s\033[0m\n" "$1"; }
die() { printf "\n\033[1;31mERROR: %s\033[0m\n" "$1"; exit 1; }

say "Checking toolchain"
command -v cargo  >/dev/null || die "cargo not found (install rustup)"
command -v cmake  >/dev/null || die "cmake not found (brew install cmake)"
command -v ollama >/dev/null || die "ollama not found"
# corepack's pnpm@latest needs Node 22+ (node:sqlite); pin a Node-20-safe pnpm.
command -v corepack >/dev/null && corepack prepare pnpm@9.15.9 --activate >/dev/null 2>&1 || true
command -v pnpm   >/dev/null || die "pnpm not found (corepack enable pnpm)"

# cidre (system-audio bindings) needs FULL Xcode, not just Command Line Tools.
if ! xcodebuild -version >/dev/null 2>&1; then
  die "Full Xcode required. Install Xcode from the App Store, then:
       sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
       sudo xcodebuild -license accept"
fi

say "Ensuring Ollama models"
for m in translategemma:4b bge-m3 qwen3.5:9b qwen2.5:14b; do
  ollama list | grep -q "${m%%:*}" || ollama pull "$m"
done

say "Preparing Python sidecar venv"
( cd "$ROOT/sidecar" && python3 -m venv .venv && source .venv/bin/activate \
    && pip install -q --upgrade pip && pip install -q -r requirements.txt )

say "Building llama-helper sidecar binary (metal)"
# Tauri externalBin expects binaries/llama-helper-<target-triple>.
( cd "$ROOT/llama-helper" && cargo build --release --features metal )
TRIPLE=$(rustc -vV | awk '/host:/{print $2}')
mkdir -p "$ROOT/frontend/src-tauri/binaries"
find "$ROOT/frontend/src-tauri/binaries" -name "llama-helper*" -delete
cp "$ROOT/target/release/llama-helper" \
   "$ROOT/frontend/src-tauri/binaries/llama-helper-$TRIPLE"
echo "   placed llama-helper-$TRIPLE"

say "Installing frontend deps"
( cd "$ROOT/frontend" && pnpm install )

say "Building Tauri app (.dmg)"
# The updater-artifact signing step at the very end exits non-zero without
# TAURI_SIGNING_PRIVATE_KEY, but that runs AFTER the .dmg is written — so we
# treat the produced .dmg as the source of truth, not the exit code.
( cd "$ROOT/frontend" && pnpm run tauri:build ) || true

DMG=$(find "$ROOT/target/release/bundle/dmg" "$ROOT/frontend/src-tauri/target" \
        -name "*.dmg" 2>/dev/null | head -1 || true)
[ -n "$DMG" ] && say "Done → $DMG" || die "Build finished but no .dmg found"
