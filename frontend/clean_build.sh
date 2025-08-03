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
    MIN_CMAKE_VERSION="3.5"
    
    # Function to compare version numbers using awk
    # Returns 0 if version1 < version2
    # Returns 1 otherwise
    version_less_than() {
        [ "$1" = "$2" ] && return 1
        echo "$1 $2" | awk '{
            split($1, v1, ".");
            split($2, v2, ".");
            for (i=1; i<=length(v1); i++) {
                if (v1[i] < v2[i]) exit 0;
                if (v1[i] > v2[i]) exit 1;
            }
            exit 0; # exit 0 if we run out of components in v1 and they were equal up to this point
                   # e.g. 3.5 is less than 3.5.1
        }'
    }

    if version_less_than "$CMAKE_VERSION" "$MIN_CMAKE_VERSION"; then
        echo "CMake version $CMAKE_VERSION is too old (required >= $MIN_CMAKE_VERSION). Updating via Homebrew..."
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

echo "Building Tauri app..."
pnpm run tauri build
sleep

