#!/bin/bash

# Exit on error
set -e

# Load the standard ignored local environment file. Release builds receive the
# same variable from GitHub Actions; local development keeps it in frontend/.env.
if [ -f ".env" ]; then
    # shellcheck disable=SC1090
    set -a
    source ".env"
    set +a
fi

# If the local env has no key, retrieve the server-owned value over authenticated
# SSH. This deliberately requires gateway host access; exposing the key over a
# public HTTP endpoint would make the registration check meaningless.
if [ -z "${MEMENTO_REGISTRATION_KEY:-}" ]; then
    GATEWAY_SSH_HOST="${MEMENTO_GATEWAY_SSH_HOST:-i167}"
    MEMENTO_REGISTRATION_KEY="$(
        ssh -o BatchMode=yes -o ConnectTimeout=8 "$GATEWAY_SSH_HOST" \
            "sudo -n sed -n 's/^MEMENTO_REGISTRATION_KEY=//p' /etc/gigatool-gateway.env 2>/dev/null || sed -n 's/^MEMENTO_REGISTRATION_KEY=//p' /etc/gigatool-gateway.env 2>/dev/null" \
            | head -n 1
    )" || true
    export MEMENTO_REGISTRATION_KEY
fi

if [ -z "${MEMENTO_REGISTRATION_KEY:-}" ] || [ "$MEMENTO_REGISTRATION_KEY" = "replace-with-development-registration-key" ]; then
    echo "Missing MEMENTO_REGISTRATION_KEY for local development."
    echo "Grant SSH access to the gateway (default host: i167), set MEMENTO_GATEWAY_SSH_HOST,"
    echo "or copy .env.example to .env and add the development key manually."
    exit 1
fi

# Lightweight validation hook used by CI and maintainers; never prints the key.
if [ "${MEMENTO_ENV_CHECK_ONLY:-0}" = "1" ]; then
    echo "Memento gateway development credential is available."
    exit 0
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

# Homebrew's Bun formula may expose `node` as a Bun compatibility shim. Prefer the
# same Node.js major used in CI when it is installed, without unlinking Bun globally.
if [[ -x "/opt/homebrew/opt/node@20/bin/node" ]]; then
    export PATH="/opt/homebrew/opt/node@20/bin:$PATH"
fi

if ! command -v node >/dev/null 2>&1 || ! node --version >/dev/null 2>&1; then
    echo "Node.js is required. On macOS run: brew install node@20"
    exit 1
fi

# Prefer Corepack so package.json pins the pnpm version. Fall back to a standalone
# pnpm binary for environments where Corepack is not bundled with Node.js.
if command -v corepack >/dev/null 2>&1; then
    # Tauri runs `pnpm dev` as a child command, so expose Corepack's pnpm shim in
    # the selected Node.js bin directory as well as using Corepack below.
    corepack enable pnpm >/dev/null 2>&1
    PNPM=(corepack pnpm)
elif command -v pnpm >/dev/null 2>&1; then
    PNPM=(pnpm)
else
    echo "pnpm is required. Install Corepack/Node.js or run: brew install pnpm"
    exit 1
fi

echo "Using Node.js $(node --version) and pnpm $("${PNPM[@]}" --version)"

# cidre builds the macOS system-audio bridge with xcodebuild. Command Line Tools alone
# are insufficient, so fail before deleting caches and reinstalling dependencies.
if [[ "$(uname -s)" == "Darwin" ]] && ! xcodebuild -version >/dev/null 2>&1; then
    echo "Full Xcode is required for the macOS Tauri build (Command Line Tools are not enough)."
    echo "Install Xcode, then run: sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer"
    exit 1
fi

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
"${PNPM[@]}" install

# Build the Next.js application first
echo "Building Next.js application..."
"${PNPM[@]}" run build

# Set environment variables for the build
echo "Setting up build environment..."

echo "Building Tauri app..."
"${PNPM[@]}" run tauri dev
sleep
