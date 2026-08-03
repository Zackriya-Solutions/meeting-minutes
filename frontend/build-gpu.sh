#!/bin/bash
# GPU-accelerated build script for Meetily
# Automatically detects and builds with optimal GPU features

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🚀 Meetily GPU-Accelerated Build Script${NC}"
echo ""

# Export CUDA flags for Linux/NVIDIA
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    export CMAKE_CUDA_ARCHITECTURES=75
    export CMAKE_CUDA_STANDARD=17
    export CMAKE_POSITION_INDEPENDENT_CODE=ON
fi

# Detect OS
if [[ "$OSTYPE" == "darwin"* ]]; then
  OS="macos"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
  OS="linux"
else
  echo -e "${RED}❌ Unsupported OS: $OSTYPE${NC}"
  exit 1
fi

# Function to check if command exists
command_exists() {
  command -v "$1" >/dev/null 2>&1
}

# Find the correct directory - we need to be in frontend root for npm commands
if [ -f "package.json" ]; then
  FRONTEND_DIR="."
elif [ -f "frontend/package.json" ]; then
  cd frontend || {
    echo -e "${RED}❌ Failed to change to frontend directory${NC}"
    exit 1
  }
  FRONTEND_DIR="frontend"
else
  echo -e "${RED}❌ Could not find package.json${NC}"
  echo -e "${RED}   Make sure you're in the project root or frontend directory${NC}"
  exit 1
fi

echo ""
echo -e "${BLUE}📦 Building Meetily...${NC}"
echo ""

# Check for pnpm or npm
if command_exists pnpm; then
  PKG_MGR="pnpm"
elif command_exists npm; then
  PKG_MGR="npm"
else
  echo -e "${RED}❌ Neither npm nor pnpm found${NC}"
  exit 1
fi

# Detect GPU feature if not already set
if [ -z "$TAURI_GPU_FEATURE" ]; then
    echo -e "${BLUE}🔍 Detecting GPU features...${NC}"
    # Run the detection script and capture output
    # We need to run it from frontend dir
    if [ "$FRONTEND_DIR" != "." ]; then
        cd "$FRONTEND_DIR"
    fi
    
    TAURI_GPU_FEATURE=$(node scripts/auto-detect-gpu.js)
    
    if [ "$FRONTEND_DIR" != "." ]; then
        cd ..
    fi
fi

if [ -n "$TAURI_GPU_FEATURE" ]; then
    echo -e "${GREEN}✅ Detected GPU feature: $TAURI_GPU_FEATURE${NC}"
    export TAURI_GPU_FEATURE
else
    echo -e "${YELLOW}⚠️ No specific GPU feature detected or forced${NC}"
fi

# Build llama-helper
echo ""
echo -e "${BLUE}🦙 Building llama-helper sidecar (release)...${NC}"

HELPER_DIR="llama-helper"
if [ ! -d "$HELPER_DIR" ]; then
    # Try to find it relative to script location
    SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
    HELPER_DIR="$SCRIPT_DIR/../llama-helper"
fi

if [ ! -d "$HELPER_DIR" ]; then
    echo -e "${RED}❌ Could not find llama-helper directory${NC}"
    exit 1
fi

# Determine llama-helper features
# Note: llama-cpp-2 does NOT support coreml, only metal/cuda/vulkan
# So for macOS Apple Silicon (which returns 'coreml' for Whisper), use 'metal' for llama-helper
HELPER_FEATURES=""
if [ -n "$TAURI_GPU_FEATURE" ]; then
    LLAMA_FEATURE="$TAURI_GPU_FEATURE"
    if [ "$LLAMA_FEATURE" = "coreml" ]; then
        LLAMA_FEATURE="metal"
        echo -e "${YELLOW}   Note: llama-cpp-2 doesn't support CoreML, using Metal instead${NC}"
    fi
    HELPER_FEATURES="--features $LLAMA_FEATURE"
fi

echo -e "   Building in $HELPER_DIR with features: ${HELPER_FEATURES:-none}"
(cd "$HELPER_DIR" && cargo build --release $HELPER_FEATURES)

if [ $? -ne 0 ]; then
    echo -e "${RED}❌ Failed to build llama-helper${NC}"
    exit 1
fi

echo -e "${GREEN}✅ llama-helper built successfully${NC}"

# Detect target triple
echo ""
echo -e "${BLUE}🎯 Detecting target triple...${NC}"
TARGET_TRIPLE=$(rustc -vV | grep "host:" | awk '{print $2}')
echo -e "   Target: $TARGET_TRIPLE"

# Copy binary
BINARIES_DIR="$FRONTEND_DIR/src-tauri/binaries"
mkdir -p "$BINARIES_DIR"

# Clean old binaries
find "$BINARIES_DIR" -name "llama-helper*" -delete

BASE_BINARY="llama-helper"
SIDECAR_BINARY="llama-helper-$TARGET_TRIPLE"

if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "win32" ]]; then
    BASE_BINARY="llama-helper.exe"
    SIDECAR_BINARY="llama-helper-$TARGET_TRIPLE.exe"
fi

# The binary is in the workspace target directory, which is one level up from frontend
# if we are in frontend dir.
WORKSPACE_ROOT="$FRONTEND_DIR/.."
SRC_PATH="$WORKSPACE_ROOT/target/release/$BASE_BINARY"
DEST_PATH="$BINARIES_DIR/$SIDECAR_BINARY"

if [ ! -f "$SRC_PATH" ]; then
    # Fallback: check if we are running from root and target is in root
    SRC_PATH="target/release/$BASE_BINARY"
fi

if [ -f "$SRC_PATH" ]; then
    cp "$SRC_PATH" "$DEST_PATH"
    echo -e "${GREEN}✅ Copied binary to $DEST_PATH${NC}"
else
    echo -e "${RED}❌ Binary not found at $SRC_PATH${NC}"
    # List contents of target/release to help debugging
    echo -e "${YELLOW}Contents of target/release:${NC}"
    ls -la "$WORKSPACE_ROOT/target/release/" || ls -la "target/release/"
    exit 1
fi

# Build using npm scripts
echo -e "${BLUE}Building complete Tauri application...${NC}"
echo ""

# NO_STRIP true due to issues with bundling appImage
NO_STRIP=true $PKG_MGR run tauri:build

if [ $? -eq 0 ]; then
  echo ""
  echo -e "${GREEN}✅ Build completed successfully!${NC}"
  echo ""
  echo -e "${GREEN}🎉 Complete Tauri application built with GPU acceleration!${NC}"
else
  echo ""
  echo -e "${RED}❌ Build failed${NC}"
  exit 1
fi

