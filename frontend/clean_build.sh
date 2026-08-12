#!/bin/bash

# Exit on error
set -e

# Load local build variables when present. The file is ignored by Git.
if [ -f ".env" ]; then
    # shellcheck disable=SC1091
    set -a
    source ".env"
    set +a
fi

# Release builds need the same gateway registration value as CI. For authorized
# maintainers, use the server-owned value when it is not supplied locally.
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
    echo "Missing MEMENTO_REGISTRATION_KEY for the local release build."
    echo "Grant SSH access to the gateway (default host: i167), set MEMENTO_GATEWAY_SSH_HOST,"
    echo "or copy .env.example to .env and add the key manually."
    exit 1
fi

# Prefer the Node.js major used in CI; Homebrew's Bun can otherwise expose an
# incompatible `node` shim.
if [[ -x "/opt/homebrew/opt/node@20/bin/node" ]]; then
    export PATH="/opt/homebrew/opt/node@20/bin:$PATH"
fi

if ! command -v node >/dev/null 2>&1 || ! node --version >/dev/null 2>&1; then
    echo "Node.js is required. On macOS run: brew install node@20"
    exit 1
fi

if command -v corepack >/dev/null 2>&1; then
    corepack enable pnpm >/dev/null 2>&1
    PNPM=(corepack pnpm)
elif command -v pnpm >/dev/null 2>&1; then
    PNPM=(pnpm)
else
    echo "pnpm is required. Install Corepack/Node.js or run: brew install pnpm"
    exit 1
fi

echo "Using Node.js $(node --version) and pnpm $("${PNPM[@]}" --version)"

# cidre requires full Xcode, not only Command Line Tools. Check before deleting
# caches or reinstalling dependencies.
if [[ "$(uname -s)" == "Darwin" ]] && ! xcodebuild -version >/dev/null 2>&1; then
    echo "Full Xcode is required for the macOS Tauri build."
    echo "Install Xcode, then run: sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer"
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

# Check and install CMake if needed
echo "Checking CMake version..."
if ! command -v cmake &> /dev/null; then
    echo "CMake not found. Installing via Homebrew..."
    brew install cmake
else
    CMAKE_VERSION=$(cmake --version | head -n1 | cut -d" " -f3)
    if ! awk -v version="$CMAKE_VERSION" 'BEGIN {
        split(version, parts, ".")
        exit !(parts[1] > 3 || (parts[1] == 3 && parts[2] >= 5))
    }'; then
        echo "CMake version $CMAKE_VERSION is too old. Updating via Homebrew..."
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
"${PNPM[@]}" install

# Build the Next.js application first
echo "Building Next.js application..."
"${PNPM[@]}" run build

# Set environment variables for the build

echo "Building Tauri app..."
# Local builds produce the .app and DMG only. Updater artifacts require the
# private signing key and are created by the protected GitHub release workflow.
TAURI_BUILD_CONFIG='{"bundle":{"createUpdaterArtifacts":false}}'
if [[ "$(uname -s)" == "Darwin" ]]; then
    LOCAL_SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY:-}"
    if [[ -z "$LOCAL_SIGNING_IDENTITY" ]]; then
        # An ad-hoc signature changes its designated requirement after every build.
        # macOS then treats the rebuilt app as a stranger to its own Keychain items,
        # producing repeated password prompts. Prefer an installed Developer ID when
        # the maintainer has one, while retaining ad-hoc signing on clean machines.
        LOCAL_SIGNING_IDENTITY="$(
            security find-identity -v -p codesigning 2>/dev/null \
                | sed -n 's/.*"\(Developer ID Application:.*\)"/\1/p' \
                | head -n 1
        )"
    fi

    if [[ -n "$LOCAL_SIGNING_IDENTITY" ]]; then
        if ! security find-identity -v -p codesigning 2>/dev/null \
            | grep -Fq "\"$LOCAL_SIGNING_IDENTITY\""; then
            echo "Configured signing identity is not available: $LOCAL_SIGNING_IDENTITY"
            exit 1
        fi
        echo "Signing local macOS build with stable identity: $LOCAL_SIGNING_IDENTITY"
        TAURI_BUILD_CONFIG="$(
            MEMENTO_LOCAL_SIGNING_IDENTITY="$LOCAL_SIGNING_IDENTITY" python3 -c \
                'import json, os; print(json.dumps({"bundle": {"createUpdaterArtifacts": False, "macOS": {"signingIdentity": os.environ["MEMENTO_LOCAL_SIGNING_IDENTITY"]}}}))'
        )"
    else
        echo "WARNING: no Developer ID identity found; using an ad-hoc signature."
        echo "         Rebuilt apps may ask again for access to existing Keychain items."
    fi
fi

"${PNPM[@]}" exec tauri build --config "$TAURI_BUILD_CONFIG"
