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

# Check and install CMake if needed
echo "Checking CMake version..."
if ! command -v cmake &> /dev/null; then
    echo "CMake not found. Installing via Homebrew..."
    brew install cmake
else
    CMAKE_VERSION=$(cmake --version | head -n1 | cut -d" " -f3)
    if [[ "$CMAKE_VERSION" < "3.5" ]]; then
        echo "CMake version $CMAKE_VERSION is too old. Updating via Homebrew..."
        brew upgrade cmake
    fi
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

# Set environment variables for the build

# Build the llama-helper sidecar in release mode before bundling, so the
# production app never ships a stale debug binary left behind by dev-gpu.sh
echo "Detecting GPU features for llama-helper..."
if [ -z "$TAURI_GPU_FEATURE" ]; then
    TAURI_GPU_FEATURE=$(node scripts/auto-detect-gpu.js)
fi

# Note: llama-cpp-2 does NOT support coreml, only metal/cuda/vulkan
# So for macOS Apple Silicon (which returns 'coreml' for Whisper), use 'metal' for llama-helper
HELPER_FEATURES=""
if [ -n "$TAURI_GPU_FEATURE" ] && [ "$TAURI_GPU_FEATURE" != "none" ]; then
    LLAMA_FEATURE="$TAURI_GPU_FEATURE"
    if [ "$LLAMA_FEATURE" = "coreml" ]; then
        LLAMA_FEATURE="metal"
        echo "Note: llama-cpp-2 doesn't support CoreML, using Metal instead"
    fi
    case "$LLAMA_FEATURE" in
        metal|cuda|vulkan)
            HELPER_FEATURES="--features $LLAMA_FEATURE"
            ;;
        *)
            # auto-detect can emit whisper-only features (hipblas/openblas)
            # that llama-helper's Cargo.toml doesn't define — build CPU-only
            echo "Note: llama-helper has no '$LLAMA_FEATURE' feature, building CPU-only"
            ;;
    esac
fi

echo "Building llama-helper sidecar (release) with features: ${HELPER_FEATURES:-none}..."
HELPER_DIR="llama-helper"
if [ ! -d "$HELPER_DIR" ]; then
    # Try to find it relative to script location
    SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
    HELPER_DIR="$SCRIPT_DIR/../llama-helper"
fi

if [ ! -d "$HELPER_DIR" ]; then
    echo "Could not find llama-helper directory"
    exit 1
fi

(cd "$HELPER_DIR" && cargo build --release $HELPER_FEATURES)

echo "Detecting target triple..."
TARGET_TRIPLE=$(rustc -vV | grep "host:" | awk '{print $2}')
echo "Target: $TARGET_TRIPLE"

BINARIES_DIR="src-tauri/binaries"
mkdir -p "$BINARIES_DIR"

# Clean old binaries
find "$BINARIES_DIR" -name "llama-helper*" -delete

BASE_BINARY="llama-helper"
SIDECAR_BINARY="llama-helper-$TARGET_TRIPLE"

if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "win32" ]]; then
    BASE_BINARY="llama-helper.exe"
    SIDECAR_BINARY="llama-helper-$TARGET_TRIPLE.exe"
fi

# The binary lands in the workspace target directory, one level up from frontend
SRC_PATH="../target/release/$BASE_BINARY"
if [ ! -f "$SRC_PATH" ]; then
    # Fallback: check if we are running from root and target is in root
    SRC_PATH="target/release/$BASE_BINARY"
fi

if [ ! -f "$SRC_PATH" ]; then
    echo "llama-helper binary not found at ../target/release/$BASE_BINARY"
    exit 1
fi

cp "$SRC_PATH" "$BINARIES_DIR/$SIDECAR_BINARY"
echo "Copied llama-helper to $BINARIES_DIR/$SIDECAR_BINARY"

echo "Building Tauri app..."
pnpm run tauri build

