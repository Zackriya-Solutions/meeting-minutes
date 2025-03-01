#!/bin/bash

# Exit immediately on error, undefined variable, or pipeline failure
set -euo pipefail

# Usage message
usage() {
  echo "Usage: $0 [log-level]"
  echo "  log-level: one of info, debug, trace (default: info)"
  exit 1
}

# Validate log level and set RUST_LOG accordingly
set_log_level() {
  local log_level="${1:-info}"
  case "$log_level" in
    info|debug|trace)
      export RUST_LOG="$log_level"
      ;;
    *)
      echo "Invalid log level: $log_level. Valid options: info, debug, trace"
      usage
      ;;
  esac
}

# Log function with timestamp
log() {
  local msg="$1"
  echo "[$(date '+%Y-%m-%d %H:%M:%S')] $msg"
}

# Check if a command exists
check_command() {
  if ! command -v "$1" &>/dev/null; then
    echo "Error: '$1' is not installed. Please install it and try again."
    exit 1
  fi
}

# Clean up function for directories
cleanup() {
  log "Cleaning up previous builds..."
  # Uncomment the following lines if you want to remove Tauri build directories
  # rm -rf target/
  # rm -rf src-tauri/target
  # rm -rf src-tauri/gen

  log "Cleaning up npm, pnp and Next.js artifacts..."
  rm -rf node_modules .next .pnp.cjs out
}

# Install dependencies
install_dependencies() {
  log "Installing dependencies..."
  pnpm install
}

# Build Next.js application
build_next() {
  log "Building Next.js application..."
  pnpm run build
}

# Build Tauri application
build_tauri() {
  log "Building Tauri app..."
  pnpm run tauri dev
}

# Main script execution
main() {
  # Check required commands
  check_command pnpm

  # Set log level from the first parameter or default to 'info'
  set_log_level "${1:-info}"

  cleanup
  install_dependencies
  build_next
  build_tauri

  # Remove or adjust this if an intentional pause is needed.
  # sleep  # This currently does nothing without a duration argument.
}

main "$@"