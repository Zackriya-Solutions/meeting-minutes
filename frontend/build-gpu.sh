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

# Build using npm scripts (which handle GPU detection automatically)
echo -e "${BLUE}Building complete Tauri application with automatic GPU detection...${NC}"
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

