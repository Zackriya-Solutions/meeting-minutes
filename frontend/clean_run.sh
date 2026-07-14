#!/bin/bash

# Exit on error
set -e

# Local development gateway credential. Release builds receive this from CI;
# developers can keep it in an ignored file instead of macOS Keychain.
DEV_ENV_FILE="${MEMENTO_DEV_ENV_FILE:-.env.local-dev}"
if [ -z "${MEMENTO_REGISTRATION_KEY:-}" ] && [ -f "$DEV_ENV_FILE" ]; then
    # shellcheck disable=SC1090
    set -a
    source "$DEV_ENV_FILE"
    set +a
fi

if [ -z "${MEMENTO_REGISTRATION_KEY:-}" ] || [ "$MEMENTO_REGISTRATION_KEY" = "replace-with-development-registration-key" ]; then
    echo "Missing MEMENTO_REGISTRATION_KEY for local development."
    echo "Copy .env.local-dev.example to .env.local-dev and add the development key."
    exit 1
fi

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

# Bypass any configured HTTP proxy for localhost. Without this, an http_proxy/
# HTTPS_PROXY with no localhost exception routes 127.0.0.1 through the proxy,
# which can't reach it — breaking the Next dev server / HMR (ChunkLoadError:
# timeout) and the app's local service calls (Ollama on :11434, etc.) with 503.
# Prepend to any existing no_proxy so we don't clobber other entries.
export no_proxy="localhost,127.0.0.1,::1${no_proxy:+,$no_proxy}"
export NO_PROXY="localhost,127.0.0.1,::1${NO_PROXY:+,$NO_PROXY}"

# Clean up previous builds
echo "Cleaning up previous builds..."
#rm -rf target/
#rm -rf src-tauri/target
#rm -rf src-tauri/gen

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
echo "Setting up build environment..."

echo "Building Tauri app..."
pnpm run tauri dev
sleep
