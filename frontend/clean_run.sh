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

# Clean up npm and pnp
echo "Cleaning up npm and pnp..."
rm -rf node_modules
rm -rf .pnp.cjs
rm -rf out
# ponytail: keep .next between runs. Next.js already invalidates changed files;
# wiping it forces a cold recompile of the whole app on every run, widening the
# window where the window is visible but still compiling. Delete it by hand
# (rm -rf .next) if you ever see a genuinely stale/corrupted cache.

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
if lsof -tiTCP:3118 -sTCP:LISTEN >/dev/null 2>&1; then
    echo "Port 3118 already in use. Stop existing dev server/Tauri process, then rerun ./clean_run.sh"
    lsof -nP -iTCP:3118 -sTCP:LISTEN || true
    exit 1
fi

if pgrep -af '/home/pc/projects/docker/meet4specs/target/debug/meetily' >/dev/null 2>&1; then
    echo "Meetily already running. Stop existing app instance, then rerun ./clean_run.sh"
    pgrep -af '/home/pc/projects/docker/meet4specs/target/debug/meetily' || true
    exit 1
fi

# ponytail: known harmless upstream GTK/tao tray-icon bug on Linux
# (tauri-apps/tao#534) — the tray builder queries a widget's scale factor
# before it's realized. Confirmed cosmetic only (same warning appears in both
# working and broken runs); just hide the noise, don't touch app code.
pnpm run tauri dev 2>&1 | grep --line-buffered -v "Gtk-CRITICAL.*gtk_widget_get_scale_factor"

