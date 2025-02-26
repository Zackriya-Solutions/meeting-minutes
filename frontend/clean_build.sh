#!/bin/bash
set -euo pipefail
IFS=$'\n\t'

# Function to print timestamped log messages
log() {
    local level="$1"
    shift
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] [$level] $*"
}

# Function to display usage instructions
usage() {
    echo "Usage: $0 [info|debug|trace]"
    exit 1
}

# Validate log level argument (default is 'info')
LOG_LEVEL=${1:-info}
case "$LOG_LEVEL" in
    info|debug|trace)
        export RUST_LOG="$LOG_LEVEL"
        ;;
    *)
        echo "Invalid log level: $LOG_LEVEL. Valid options: info, debug, trace"
        usage
        ;;
esac

# Check for pnpm dependency
if ! command -v pnpm &>/dev/null; then
    log "ERROR" "pnpm is not installed. Please install pnpm and try again."
    exit 1
fi

# Function to remove directories if they exist
cleanup() {
    local paths=("target/" "src-tauri/target" "src-tauri/gen" "node_modules" ".next" ".pnp.cjs" "out")
    for path in "${paths[@]}"; do
        if [ -e "$path" ]; then
            log "INFO" "Removing $path"
            rm -rf "$path"
        fi
    done
}

# Main script execution
log "INFO" "Starting cleanup of previous builds and dependencies..."
cleanup

log "INFO" "Installing dependencies..."
pnpm install

log "INFO" "Building Next.js application..."
pnpm run build

log "INFO" "Building Tauri app..."
pnpm run tauri build

log "INFO" "Build process completed successfully."