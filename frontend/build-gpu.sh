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

# Detect GPU and build features
if [[ "$OS" == "macos" ]]; then
    echo -e "${GREEN}✅ macOS detected${NC}"
    echo -e "${GREEN}   Metal GPU acceleration will be enabled by default${NC}"
    FEATURES=""

    # Check if we should enable CoreML
    if [[ $(uname -m) == "arm64" ]]; then
        echo -e "${GREEN}   Apple Silicon detected - CoreML available${NC}"
        read -p "Enable CoreML acceleration? (y/N): " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            FEATURES="coreml"
            echo -e "${GREEN}   CoreML will be enabled${NC}"
        fi
    fi

elif [[ "$OS" == "linux" ]]; then
    echo -e "${BLUE}🔍 Detecting GPU capabilities...${NC}"

    # Check for NVIDIA GPU
    if command_exists nvidia-smi; then
        echo -e "${GREEN}✅ NVIDIA GPU detected${NC}"
        nvidia-smi --query-gpu=name --format=csv,noheader | head -n1

        # Check if CUDA is properly installed
        if [[ -n "$CUDA_PATH" ]] || command_exists nvcc; then
            FEATURES="cuda"
            echo -e "${GREEN}   Building with CUDA acceleration${NC}"
        else
            echo -e "${YELLOW}   CUDA toolkit not found - falling back to CPU${NC}"
            echo -e "${YELLOW}   Install CUDA Toolkit to enable GPU acceleration${NC}"
            FEATURES=""
        fi

    # Check for AMD GPU
    elif command_exists rocm-smi; then
        echo -e "${GREEN}✅ AMD GPU detected${NC}"

        # Check if ROCm is properly installed
        if [[ -n "$ROCM_PATH" ]] || command_exists hipcc; then
            FEATURES="hipblas"
            echo -e "${GREEN}   Building with AMD ROCm (HIP) acceleration${NC}"
        else
            echo -e "${YELLOW}   ROCm not found - falling back to CPU${NC}"
            echo -e "${YELLOW}   Install ROCm to enable GPU acceleration${NC}"
            FEATURES=""
        fi

    # Check for Vulkan support (fallback for Intel/other GPUs)
    elif command_exists vulkaninfo; then
        echo -e "${BLUE}ℹ️  Vulkan support detected${NC}"

        # Check if required environment variables are set
        if [[ -n "$VULKAN_SDK" ]] && [[ -n "$BLAS_INCLUDE_DIRS" ]]; then
            FEATURES="vulkan"
            echo -e "${GREEN}   Building with Vulkan acceleration${NC}"
        else
            echo -e "${YELLOW}   Missing required environment variables:${NC}"
            [[ -z "$VULKAN_SDK" ]] && echo -e "${YELLOW}   - VULKAN_SDK not set${NC}"
            [[ -z "$BLAS_INCLUDE_DIRS" ]] && echo -e "${YELLOW}   - BLAS_INCLUDE_DIRS not set${NC}"
            echo -e "${YELLOW}   Falling back to CPU optimization (OpenBLAS)${NC}"
            FEATURES=""
        fi

    else
        echo -e "${BLUE}ℹ️  No GPU detected${NC}"
        echo -e "${BLUE}   Building with CPU optimization (OpenBLAS)${NC}"
        FEATURES=""
    fi
fi

# Build command
cd frontend/src-tauri || { echo -e "${RED}❌ Failed to change to frontend/src-tauri directory${NC}"; exit 1; }

echo ""
echo -e "${BLUE}📦 Building Meetily...${NC}"

if [[ -z "$FEATURES" ]]; then
    cargo build --release
else
    cargo build --release --features "$FEATURES"
fi

if [ $? -eq 0 ]; then
    echo ""
    echo -e "${GREEN}✅ Build completed successfully!${NC}"
    echo ""
    echo -e "${BLUE}Build configuration:${NC}"
    echo -e "  OS: $OS"
    echo -e "  Features: ${FEATURES:-default (CPU optimized)}"
    echo ""
    echo -e "${GREEN}🎉 You can now run Meetily with GPU acceleration!${NC}"
else
    echo ""
    echo -e "${RED}❌ Build failed${NC}"
    exit 1
fi