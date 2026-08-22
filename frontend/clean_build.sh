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

# Check CMake
if ! command -v cmake &> /dev/null; then
    echo "CMake not found. Install it first: sudo apt install cmake (Linux) / brew install cmake (macOS)"
    exit 1
fi

# Clean up previous builds
echo "Cleaning up previous builds..."
rm -rf target/
rm -rf src-tauri/target
rm -rf src-tauri/gen

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

# Choose Tauri config based on signing environment

echo "Building Tauri app..."
if [ -n "$TAURI_SIGNING_PRIVATE_KEY" ]; then
    echo "Signing key detected. Using default Tauri config with updater artifacts enabled."
    pnpm tauri build
else
    echo "No TAURI_SIGNING_PRIVATE_KEY detected. Using local Tauri override: src-tauri/tauri.local.conf.json"
    echo "Updater signing artifacts disabled for local build."
    pnpm tauri build --config src-tauri/tauri.local.conf.json
fi

