#!/bin/bash

# Multi-platform Docker build script for Whisper Server and Meeting App
# Supports both CPU-only and GPU-enabled builds across multiple architectures
#
# ⚠️  AUDIO PROCESSING WARNING:
# Docker containers with insufficient resources will drop audio chunks when
# the processing queue becomes full (MAX_AUDIO_QUEUE_SIZE=10, lib.rs:54).
# Ensure containers have adequate memory (8GB+) and CPU allocation.
# Monitor logs for "Dropped old audio chunk" messages (lib.rs:330).

set -e

# Configuration
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WHISPER_PROJECT_NAME="whisper-server"
APP_PROJECT_NAME="meetily-backend"
REGISTRY=${REGISTRY:-""}
PUSH=${PUSH:-false}
# Default to current platform for local builds, multi-platform for registry pushes
DEFAULT_PLATFORMS="linux/$(uname -m | sed 's/x86_64/amd64/' | sed 's/aarch64/arm64/')"
PLATFORMS=${PLATFORMS:-$DEFAULT_PLATFORMS}
BUILD_ARGS=${BUILD_ARGS:-""}

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Ensure required directories exist
ensure_directories() {
    # Create data directory for database if it doesn't exist
    if [ ! -d "$SCRIPT_DIR/data" ]; then
        log_info "Creating data directory for database..."
        mkdir -p "$SCRIPT_DIR/data"
        chmod 755 "$SCRIPT_DIR/data"
        log_info "✓ Data directory created"
    fi
    
    # Create models directory if it doesn't exist
    if [ ! -d "$SCRIPT_DIR/models" ]; then
        log_info "Creating models directory..."
        mkdir -p "$SCRIPT_DIR/models"
        chmod 755 "$SCRIPT_DIR/models"
        log_info "✓ Models directory created"
    fi
    
    # Create config directory if it doesn't exist
    if [ ! -d "$SCRIPT_DIR/config" ]; then
        mkdir -p "$SCRIPT_DIR/config"
        chmod 755 "$SCRIPT_DIR/config"
    fi
}

# Ensure directories exist on script start
ensure_directories

# Platform detection for macOS support
DETECTED_OS=$(uname -s)
IS_MACOS=false
if [[ "$DETECTED_OS" == "Darwin" ]]; then
    IS_MACOS=true
    log_info "macOS detected - will use macOS-optimized configurations"
fi

# Error handling
handle_error() {
    log_error "$1"
    exit 1
}

show_help() {
    cat << EOF
Multi-platform Whisper Server and Meeting App Docker Builder

Usage: $0 [OPTIONS] [BUILD_TYPE]

BUILD_TYPE:
  cpu           Build whisper server CPU-only + meeting app (default)
  gpu           Build whisper server GPU-enabled + meeting app
  macos         Build whisper server macOS-optimized + meeting app (auto-selected on macOS)
  both          Build both whisper server versions + meeting app
  
OPTIONS:
  -r, --registry REGISTRY    Docker registry (e.g., ghcr.io/user)
  -p, --push                 Push images to registry
  -t, --tag TAG              Custom tag (default: auto-generated)
  --platforms PLATFORMS      Target platforms (default: current platform)
  --build-args ARGS          Additional build arguments
  --no-cache                 Build without cache
  --dry-run                  Show commands without executing
  -h, --help                 Show this help

Examples:
  # Build whisper CPU version + meeting app for current platform
  $0 cpu
  
  # Build whisper GPU version + meeting app
  $0 gpu
  
  # Build both whisper versions + meeting app
  $0 both
  
  # Build GPU version for multiple platforms (requires --push)
  $0 gpu --platforms linux/amd64,linux/arm64 --push
  
  # Build both versions and push to registry
  $0 both --registry ghcr.io/myuser --push
  
  # Build with custom CUDA version
  $0 gpu --build-args "CUDA_VERSION=12.1.1"

Note: The meeting app is always built alongside the whisper server as they work as a package.

Environment Variables:
  REGISTRY      Docker registry prefix
  PUSH          Push to registry (true/false)
  PLATFORMS     Target platforms
  BUILD_ARGS    Additional build arguments

EOF
}

# Function to check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."
    
    # Check Docker
    if ! command -v docker >/dev/null 2>&1; then
        log_error "Docker is not installed or not in PATH"
        exit 1
    fi
    
    # Check Docker Buildx
    if ! docker buildx version >/dev/null 2>&1; then
        log_error "Docker Buildx is not available"
        log_error "Please install Docker Desktop or enable Buildx"
        exit 1
    fi
    
    # Check if buildx builder exists
    if ! docker buildx ls | grep -q "whisper-builder"; then
        log_info "Creating multi-platform builder..."
        docker buildx create --name whisper-builder --platform "$PLATFORMS" --use
    else
        log_info "Using existing whisper-builder"
        docker buildx use whisper-builder
    fi
    
    # Check whisper.cpp directory
    if [ ! -d "$SCRIPT_DIR/whisper.cpp" ]; then
        log_error "whisper.cpp directory not found"
        log_error "Please ensure whisper.cpp is cloned in the current directory"
        exit 1
    fi
    
    log_info "Prerequisites check passed"
}


# log_info "Updating git submodules..."
# git submodule update --init --recursive || handle_error "Failed to update git submodules"


log_info "Changing to whisper.cpp directory..."
cd whisper.cpp || handle_error "Failed to change to whisper.cpp directory"

log_info "Checking for custom server directory..."
if [ ! -d "../whisper-custom/server" ]; then
    handle_error "Directory '../whisper-custom/server' not found. Please make sure the custom server files exist"
fi

log_info "Updating git submodules..."
git submodule update --init --recursive || handle_error "Failed to update git submodules"


log_info "Copying custom server files..."
cp -r ../whisper-custom/server/* "examples/server/" || handle_error "Failed to copy custom server files"
log_info "Custom server files copied successfully"

log_info "Verifying server files..."
ls "examples/server/" || handle_error "Failed to list server files"

log_info "Returning to original directory..."
cd "$SCRIPT_DIR" || handle_error "Failed to return to original directory"

# Function to generate image tag
generate_tag() {
    local build_type="$1"
    local custom_tag="$2"
    
    if [ -n "$custom_tag" ]; then
        echo "$custom_tag"
        return
    fi
    
    local tag=""
    local timestamp=$(date +%Y%m%d)
    
    # Get git commit hash if available
    local git_hash=""
    if git rev-parse --short HEAD >/dev/null 2>&1; then
        git_hash="-$(git rev-parse --short HEAD)"
    fi
    
    case "$build_type" in
        "cpu")
            tag="cpu-${timestamp}${git_hash}"
            ;;
        "gpu")
            tag="gpu-${timestamp}${git_hash}"
            ;;
        *)
            tag="${build_type}-${timestamp}${git_hash}"
            ;;
    esac
    
    echo "$tag"
}

# Function to build Docker image
build_image() {
    local build_type="$1"
    local tag="$2"
    local dockerfile=""
    local full_tag=""
    local project_name=""
    local build_args_array=()
    
    # Determine dockerfile and project name
    case "$build_type" in
        "cpu")
            dockerfile="Dockerfile.server-cpu"
            project_name="$WHISPER_PROJECT_NAME"
            ;;
        "gpu")
            dockerfile="Dockerfile.server-gpu"
            project_name="$WHISPER_PROJECT_NAME"
            ;;
        "macos")
            dockerfile="Dockerfile.server-macos"
            project_name="$WHISPER_PROJECT_NAME"
            ;;
        "app")
            dockerfile="Dockerfile.app"
            project_name="$APP_PROJECT_NAME"
            ;;
        *)
            log_error "Unknown build type: $build_type"
            return 1
            ;;
    esac
    
    # Construct full tag
    if [ -n "$REGISTRY" ]; then
        full_tag="${REGISTRY}/${project_name}:${tag}"
    else
        full_tag="${project_name}:${tag}"
    fi
    
    # Parse build arguments
    if [ -n "$BUILD_ARGS" ]; then
        IFS=' ' read -ra ADDR <<< "$BUILD_ARGS"
        for arg in "${ADDR[@]}"; do
            build_args_array+=("--build-arg" "$arg")
        done
    fi
    
    # Build command
    local build_cmd=(
        "docker" "buildx" "build"
        "--platform" "$PLATFORMS"
        "--file" "$dockerfile"
        "--tag" "$full_tag"
        "${build_args_array[@]}"
    )
    
    # Add cache options
    if [ "$NO_CACHE" = "true" ]; then
        build_cmd+=("--no-cache")
    fi
    
    # Add push/load option - only use --load for single platform builds
    if [ "$PUSH" = "true" ]; then
        build_cmd+=("--push")
    else
        # Check if building for multiple platforms
        if [[ "$PLATFORMS" == *","* ]]; then
            log_warn "Multi-platform build detected without --push"
            log_warn "Multi-platform builds cannot be loaded locally"
            log_warn "Either use --push or specify single platform with --platforms"
            return 1
        else
            build_cmd+=("--load")
        fi
    fi
    
    # Add context
    build_cmd+=(".")
    
    log_info "Building $build_type image: $full_tag"
    log_info "Platforms: $PLATFORMS"
    log_info "Dockerfile: $dockerfile"
    
    if [ "$DRY_RUN" = "true" ]; then
        log_info "DRY RUN - Command would be:"
        echo "${build_cmd[@]}"
        return 0
    fi
    
    # Execute build
    if "${build_cmd[@]}"; then
        log_info "✓ Successfully built: $full_tag"
        
        # Also tag as latest for this build type
        local latest_tag=""
        if [ -n "$REGISTRY" ]; then
            latest_tag="${REGISTRY}/${project_name}:${build_type}"
        else
            latest_tag="${project_name}:${build_type}"
        fi
        
        if [ "$PUSH" = "true" ]; then
            log_info "Tagging as latest: $latest_tag"
            docker buildx build \
                --platform "$PLATFORMS" \
                --file "$dockerfile" \
                --tag "$latest_tag" \
                "${build_args_array[@]}" \
                --push \
                .
        else
            # For local builds, create a simple tag without timestamp
            log_info "Tagging locally: $latest_tag"
            docker tag "$full_tag" "$latest_tag"
        fi
        
        return 0
    else
        log_error "✗ Failed to build: $full_tag"
        return 1
    fi
}

# Main function
main() {
    local build_type="cpu"
    local custom_tag=""
    
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            -r|--registry)
                REGISTRY="$2"
                shift 2
                ;;
            -p|--push)
                PUSH=true
                shift
                ;;
            -t|--tag)
                custom_tag="$2"
                shift 2
                ;;
            --platforms)
                PLATFORMS="$2"
                shift 2
                ;;
            --build-args)
                BUILD_ARGS="$2"
                shift 2
                ;;
            --no-cache)
                NO_CACHE=true
                shift
                ;;
            --dry-run)
                DRY_RUN=true
                shift
                ;;
            -h|--help)
                show_help
                exit 0
                ;;
            cpu|gpu|macos|both)
                build_type="$1"
                shift
                ;;
            *)
                log_error "Unknown option: $1"
                show_help
                exit 1
                ;;
        esac
    done
    
    # Auto-detect macOS and adjust build type if needed
    if [[ "$IS_MACOS" == "true" && "$build_type" == "cpu" ]]; then
        log_info "macOS detected - switching from CPU to macOS-optimized build"
        build_type="macos"
    elif [[ "$IS_MACOS" == "true" && "$build_type" == "gpu" ]]; then
        log_warn "GPU build requested on macOS - switching to macOS-optimized (CPU-only) build"
        build_type="macos"
    fi
    
    log_info "=== Whisper Server Docker Builder ==="
    log_info "Build type: $build_type"
    log_info "Registry: ${REGISTRY:-<none>}"
    log_info "Platforms: $PLATFORMS"
    log_info "Push: $PUSH"
    
    # Check prerequisites
    check_prerequisites
    
    # Build images - always build meeting app alongside whisper server
    case "$build_type" in
        "cpu")
            local whisper_tag=$(generate_tag "cpu" "$custom_tag")
            local app_tag=$(generate_tag "app" "$custom_tag")
            
            log_info "Building whisper server (CPU) + meeting app..."
            build_image "cpu" "$whisper_tag"
            build_image "app" "$app_tag"
            ;;
        "gpu")
            local whisper_tag=$(generate_tag "gpu" "$custom_tag")
            local app_tag=$(generate_tag "app" "$custom_tag")
            
            log_info "Building whisper server (GPU) + meeting app..."
            build_image "gpu" "$whisper_tag"
            build_image "app" "$app_tag"
            ;;
        "macos")
            local whisper_tag=$(generate_tag "macos" "$custom_tag")
            local app_tag=$(generate_tag "app" "$custom_tag")
            
            log_info "Building whisper server (macOS-optimized) + meeting app..."
            build_image "macos" "$whisper_tag"
            build_image "app" "$app_tag"
            ;;
        "both")
            local cpu_tag=$(generate_tag "cpu" "$custom_tag")
            local gpu_tag=$(generate_tag "gpu" "$custom_tag")
            local app_tag=$(generate_tag "app" "$custom_tag")
            
            log_info "Building both whisper server versions + meeting app..."
            build_image "cpu" "$cpu_tag"
            build_image "gpu" "$gpu_tag"
            build_image "app" "$app_tag"
            ;;
        *)
            log_error "Invalid build type: $build_type"
            show_help
            exit 1
            ;;
    esac
    
    log_info "=== Build Complete ==="
    
    # Show built images
    if [ "$DRY_RUN" != "true" ] && [ "$PUSH" != "true" ]; then
        log_info "Built images:"
        # Always show both whisper and app images since they're built together
        docker images "${WHISPER_PROJECT_NAME}" --format "table {{.Repository}}:{{.Tag}}\t{{.Size}}\t{{.CreatedAt}}" 2>/dev/null || true
        docker images "${APP_PROJECT_NAME}" --format "table {{.Repository}}:{{.Tag}}\t{{.Size}}\t{{.CreatedAt}}" 2>/dev/null || true
    fi
}

# Execute main function
main "$@"