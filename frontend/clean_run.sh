#!/bin/bash

# Exit on error
set -e

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
rm -rf .next
rm -rf .pnp.cjs
rm -rf out

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

exec pnpm run tauri dev

