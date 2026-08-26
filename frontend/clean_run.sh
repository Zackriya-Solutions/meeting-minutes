#!/bin/bash

# Exit on error (pipefail so the trailing `| grep -v` below doesn't mask failures)
set -e
set -o pipefail

# Add log level selector with default to INFO
LOG_LEVEL=${1:-info}

case $LOG_LEVEL in
    info|debug|trace)
        export RUST_LOG=$LOG_LEVEL
        ;;
    *)
        echo "Invalid log level: $LOG_LEVEL. Valid options: info, debug, trace"
        exit 1
        ;;
esac

# Clean up previous builds
echo "Cleaning up previous builds..."
#rm -rf target/
#rm -rf src-tauri/target
#rm -rf src-tauri/gen

# Clean up npm, pnp and next
echo "Cleaning up npm, pnp and next..."
rm -rf node_modules
rm -rf .pnp.cjs
rm -rf out
rm -rf .next
# ponytail: .next MUST go whenever node_modules is reinstalled — a fresh
# node_modules can produce different webpack runtime IDs, so a stale .next
# serves chunks the new build doesn't recognize (ChunkLoadError). Keeping
# .next is only safe when node_modules stays untouched between runs.

echo "Installing dependencies..."
pnpm install

# Build the Next.js application first
echo "Building Next.js application..."
pnpm run build

# Set environment variables for the build
echo "Setting up build environment..."

echo "Preparing llama-helper sidecar..."
TARGET_TRIPLE=$(rustc -vV | sed -n 's/^host: //p')
EXE_SUFFIX=""
if [[ "$TARGET_TRIPLE" == *windows* ]]; then
    EXE_SUFFIX=".exe"
fi

cargo build -p llama-helper --manifest-path ../Cargo.toml
mkdir -p src-tauri/binaries
cp "../target/debug/llama-helper${EXE_SUFFIX}" "src-tauri/binaries/llama-helper-${TARGET_TRIPLE}${EXE_SUFFIX}"
chmod +x "src-tauri/binaries/llama-helper-${TARGET_TRIPLE}${EXE_SUFFIX}" 2>/dev/null || true

echo "Building Tauri app..."

# Kill any leftover instance (app, orphaned webview, or dev server) from a
# previous run before starting a new one, so a stale process never leaves the
# window stuck loading against a dead dev server.
echo "Stopping any previous Meet4Specs/dev-server instance..."
pkill -9 -f 'target/debug/meet4specs' 2>/dev/null || true
pkill -9 -f 'target/release/meet4specs' 2>/dev/null || true
pkill -9 -f 'next dev -p 3118' 2>/dev/null || true
pkill -9 -f 'WebKitNetworkProcess' 2>/dev/null || true
pkill -9 -f 'WebKitWebProcess' 2>/dev/null || true

# Condition-based wait instead of a fixed sleep: a killed process's socket
# isn't always released by the kernel instantly, so poll port 3118 until it's
# actually free (bounded) rather than guessing a fixed delay is long enough.
for _ in $(seq 1 20); do
    port_pids=$(lsof -tiTCP:3118 2>/dev/null || true)
    if [ -z "$port_pids" ]; then
        break
    fi
    echo "$port_pids" | xargs -r kill -9 2>/dev/null || true
    sleep 0.5
done

if lsof -tiTCP:3118 >/dev/null 2>&1; then
    echo "Port 3118 is still held after 10s of cleanup attempts:"
    lsof -nP -iTCP:3118 || true
    echo "Stop it manually, then rerun ./clean_run.sh"
    exit 1
fi

# ponytail: known harmless upstream GTK/tao tray-icon bug on Linux
# (tauri-apps/tao#534) — the tray builder queries a widget's scale factor
# before it's realized. Confirmed cosmetic only (same warning appears in both
# working and broken runs); just hide the noise, don't touch app code.
pnpm run tauri dev 2>&1 | grep --line-buffered -v "Gtk-CRITICAL.*gtk_widget_get_scale_factor"

