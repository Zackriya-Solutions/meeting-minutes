#!/usr/bin/env bash
set -euo pipefail

APP_NAME="MeetilyHeb"
BUNDLE_ID="com.elad.meetilyheb"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FRONTEND_DIR="$REPO_ROOT/frontend"
APP_DEST="/Applications/${APP_NAME}.app"
BUILD_APP="$REPO_ROOT/target/release/bundle/macos/${APP_NAME}.app"
PULL=0
BUILD=1
LAUNCH=0
FORCE_QUIT=0

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Build and install ${APP_NAME} on this Mac.

Options:
  --pull          Fetch and fast-forward the current branch before building.
  --skip-build    Install the existing built app from target/release/bundle/macos.
  --launch        Launch ${APP_NAME} after installing.
  --force-quit    Kill ${APP_NAME} if it does not quit normally.
  --dest PATH     Install to PATH instead of ${APP_DEST}.
  -h, --help      Show this help.

Examples:
  scripts/update-local-macos.sh
  scripts/update-local-macos.sh --pull --launch
  scripts/update-local-macos.sh --skip-build --dest "\$HOME/Applications/${APP_NAME}.app"
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --pull)
      PULL=1
      shift
      ;;
    --skip-build)
      BUILD=0
      shift
      ;;
    --launch)
      LAUNCH=1
      shift
      ;;
    --force-quit)
      FORCE_QUIT=1
      shift
      ;;
    --dest)
      APP_DEST="${2:?missing destination path}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This installer is macOS-only." >&2
  exit 1
fi

export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/Users/elad.moshe/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin:/Users/elad.moshe/.cache/codex-runtimes/codex-primary-runtime/dependencies/bin:$PATH"

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

require_command git
require_command cargo
require_command rustc
require_command pnpm
require_command ditto
require_command osascript

if [[ "$BUILD" -eq 1 ]]; then
  require_command cmake
fi

cd "$REPO_ROOT"

if [[ "$PULL" -eq 1 ]]; then
  if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "Refusing to pull with uncommitted changes. Commit/stash first, or rerun without --pull." >&2
    exit 1
  fi

  current_branch="$(git symbolic-ref --quiet --short HEAD || true)"
  if [[ -z "$current_branch" ]]; then
    echo "Refusing to pull from detached HEAD." >&2
    exit 1
  fi

  git fetch --prune
  git merge --ff-only "@{u}"
fi

target_triple="$(rustc -vV | awk '/^host:/ { print $2 }')"
sidecar="$FRONTEND_DIR/src-tauri/binaries/llama-helper-${target_triple}"

if [[ "$BUILD" -eq 1 ]]; then
  echo "Building llama-helper sidecar for ${target_triple}..."
  cargo build -p llama-helper --release --features metal
  mkdir -p "$FRONTEND_DIR/src-tauri/binaries"
  cp "$REPO_ROOT/target/release/llama-helper" "$sidecar"
  chmod +x "$sidecar"

  echo "Building ${APP_NAME}.app..."
  (cd "$FRONTEND_DIR" && pnpm exec tauri build --bundles app)
fi

if [[ ! -d "$BUILD_APP" ]]; then
  echo "Built app not found: $BUILD_APP" >&2
  exit 1
fi

echo "Quitting any running ${APP_NAME} instance..."
osascript -e "tell application id \"${BUNDLE_ID}\" to quit" >/dev/null 2>&1 || true

for _ in {1..20}; do
  if ! pgrep -x "$APP_NAME" >/dev/null 2>&1 && ! pgrep -x "meetilyheb" >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done

if pgrep -x "$APP_NAME" >/dev/null 2>&1 || pgrep -x "meetilyheb" >/dev/null 2>&1; then
  if [[ "$FORCE_QUIT" -eq 1 ]]; then
    pkill -x "$APP_NAME" >/dev/null 2>&1 || true
    pkill -x "meetilyheb" >/dev/null 2>&1 || true
  else
    echo "${APP_NAME} is still running. Quit it or rerun with --force-quit." >&2
    exit 1
  fi
fi

install_parent="$(dirname "$APP_DEST")"
mkdir -p "$install_parent"

backup="${APP_DEST}.previous"
rm -rf "$backup"
if [[ -d "$APP_DEST" ]]; then
  mv "$APP_DEST" "$backup"
fi

echo "Installing $BUILD_APP -> $APP_DEST"
if ! ditto "$BUILD_APP" "$APP_DEST"; then
  rm -rf "$APP_DEST"
  if [[ -d "$backup" ]]; then
    mv "$backup" "$APP_DEST"
  fi
  echo "Install failed; restored previous app if one existed." >&2
  exit 1
fi

rm -rf "$backup"
xattr -dr com.apple.quarantine "$APP_DEST" >/dev/null 2>&1 || true
codesign --verify --deep --strict "$APP_DEST"

echo "Installed ${APP_NAME}: $APP_DEST"

if [[ "$LAUNCH" -eq 1 ]]; then
  open "$APP_DEST"
fi
