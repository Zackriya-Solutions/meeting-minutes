#!/bin/bash
# GPU-accelerated development script for Meetily
# Automatically detects and runs in development mode with optimal GPU features

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

echo -e "${BLUE}🚀 Meetily GPU-Accelerated Development Mode${NC}"
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

# Find the correct directory - we need to be in frontend root for npm commands
if [ -f "package.json" ]; then
    FRONTEND_DIR="."
elif [ -f "frontend/package.json" ]; then
    cd frontend
    FRONTEND_DIR="frontend"
else
    echo -e "${RED}❌ Could not find package.json${NC}"
    echo -e "${RED}   Make sure you're in the project root or frontend directory${NC}"
    exit 1
fi

echo ""
echo -e "${BLUE}📦 Starting Meetily in development mode...${NC}"
echo ""

# Set up GPU features as cargo flags
if [[ -z "$FEATURES" ]]; then
    echo -e "${CYAN}Running: cargo tauri dev${NC}"
    export CARGO_BUILD_FEATURES=""
else
    echo -e "${CYAN}Running: cargo tauri dev with features: $FEATURES${NC}"
    export CARGO_BUILD_FEATURES="--features $FEATURES"
fi

# Run tauri dev - use npm/pnpm tauri instead of cargo tauri
if [[ -z "$FEATURES" ]]; then
    if command_exists pnpm; then
        pnpm tauri dev
    elif command_exists npm; then
        npm run tauri dev
    else
        echo -e "${RED}❌ Neither npm nor pnpm found${NC}"
        exit 1
    fi
else
    # When features are needed, we need to pass them to cargo
    # We'll set an environment variable that cargo will pick up
    export CARGO_FEATURES="--features $FEATURES"
    if command_exists pnpm; then
        pnpm tauri dev -- -- --features "$FEATURES"
    elif command_exists npm; then
        npm run tauri dev -- -- --features "$FEATURES"
    else
        echo -e "${RED}❌ Neither npm nor pnpm found${NC}"
        exit 1
    fi
fi

if [ $? -eq 0 ]; then
    echo ""
    echo -e "${GREEN}✅ Development server stopped cleanly${NC}"
else
    echo ""
    echo -e "${RED}❌ Development server encountered an error${NC}"
    exit 1
fi