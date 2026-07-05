#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UPDATER="$REPO_ROOT/scripts/update-local-macos.sh"
LABEL="com.elad.meetilyheb.local-updater"
PLIST="$HOME/Library/LaunchAgents/${LABEL}.plist"
LOG_DIR="$HOME/Library/Logs/MeetilyHeb"
INTERVAL_SECONDS="${1:-86400}"

usage() {
  cat <<EOF
Usage: $(basename "$0") [interval_seconds]

Install a per-user macOS LaunchAgent that runs:
  $UPDATER --pull

Default interval: 86400 seconds.

The updater refuses to pull over uncommitted changes and requires the current
Git branch to have an upstream configured.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "LaunchAgent installation is macOS-only." >&2
  exit 1
fi

if [[ ! "$INTERVAL_SECONDS" =~ ^[0-9]+$ || "$INTERVAL_SECONDS" -lt 300 ]]; then
  echo "Interval must be an integer >= 300 seconds." >&2
  exit 2
fi

if [[ ! -x "$UPDATER" ]]; then
  echo "Updater script is not executable: $UPDATER" >&2
  exit 1
fi

cd "$REPO_ROOT"
if ! git rev-parse --abbrev-ref --symbolic-full-name "@{u}" >/dev/null 2>&1; then
  echo "Current branch has no upstream. Configure one before enabling --pull automation." >&2
  exit 1
fi

mkdir -p "$HOME/Library/LaunchAgents" "$LOG_DIR"

cat > "$PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>${LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>${UPDATER}</string>
    <string>--pull</string>
  </array>
  <key>StartInterval</key>
  <integer>${INTERVAL_SECONDS}</integer>
  <key>RunAtLoad</key>
  <true/>
  <key>StandardOutPath</key>
  <string>${LOG_DIR}/local-updater.log</string>
  <key>StandardErrorPath</key>
  <string>${LOG_DIR}/local-updater.err.log</string>
  <key>WorkingDirectory</key>
  <string>${REPO_ROOT}</string>
</dict>
</plist>
EOF

launchctl bootout "gui/$(id -u)" "$PLIST" >/dev/null 2>&1 || true
launchctl bootstrap "gui/$(id -u)" "$PLIST"
launchctl enable "gui/$(id -u)/${LABEL}"

echo "Installed and loaded LaunchAgent: $PLIST"
echo "Logs:"
echo "  $LOG_DIR/local-updater.log"
echo "  $LOG_DIR/local-updater.err.log"
