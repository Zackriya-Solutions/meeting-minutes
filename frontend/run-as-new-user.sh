#!/bin/bash
#
# run-as-new-user.sh — Reset Meetily's per-user state to a clean "first install"
# and launch the dev build, so you can exercise the onboarding / first-run flow.
#
# macOS only (the supported dev target). All existing state is MOVED (not deleted)
# to a timestamped backup under ~/Library, so every reset is fully restorable.
#
# What counts as "per-user state" for bundle id com.meetily.ai (+ legacy "meetily"):
#   ~/Library/Application Support/{com.meetily.ai,meetily}   (DB, prefs, analytics, models)
#   ~/Library/Preferences/{com.meetily.ai,meetily}.plist     (webview prefs)
#   ~/Library/Caches/{com.meetily.ai,meetily}                (caches)
#   ~/Library/WebKit/{com.meetily.ai,meetily}                (webview localStorage/IndexedDB)
#   Keychain service "meetily.gateway" (device-id, install-token — managed cloud identity)
#
# Usage:
#   ./run-as-new-user.sh [options]
#
# Options:
#   --keep-models     Preserve the downloaded models dir (~GBs) across the reset.
#                     Faster (no re-download) but not a 100% authentic first-run.
#   --reset-perms     Also reset macOS TCC permissions (mic, screen recording,
#                     notifications) so the app re-prompts. Requires re-granting.
#   --no-run          Reset state only; do not launch the app.
#   --restore         Restore the most recent backup (undo a prior reset) and exit.
#   --log LEVEL       Rust log level: info (default) | debug | trace.
#   -h, --help        Show this help.
#
set -euo pipefail

APP_ID="com.meetily.ai"
LEGACY_ID="meetily"
KC_SERVICE="meetily.gateway"
LIB="$HOME/Library"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

KEEP_MODELS=0
RESET_PERMS=0
DO_RUN=1
DO_RESTORE=0
LOG_LEVEL="info"

# ---- parse args -------------------------------------------------------------
while [ $# -gt 0 ]; do
  case "$1" in
    --keep-models) KEEP_MODELS=1 ;;
    --reset-perms) RESET_PERMS=1 ;;
    --no-run)      DO_RUN=0 ;;
    --restore)     DO_RESTORE=1 ;;
    --log)         LOG_LEVEL="${2:?--log needs a level}"; shift ;;
    info|debug|trace) LOG_LEVEL="$1" ;;   # positional convenience, like clean_run.sh
    -h|--help)     awk 'NR==1{next} /^#/{sub(/^# ?/,""); print; next} {exit}' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "Unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

case "$LOG_LEVEL" in info|debug|trace) ;; *) echo "Invalid log level: $LOG_LEVEL" >&2; exit 2 ;; esac

if [ "$(uname)" != "Darwin" ]; then
  echo "This script targets macOS only." >&2; exit 1
fi

# Paths that hold per-user state, as "SUBDIR:leafname" pairs under ~/Library.
STATE_ITEMS=(
  "Application Support:$APP_ID"
  "Application Support:$LEGACY_ID"
  "Preferences:$APP_ID.plist"
  "Preferences:$LEGACY_ID.plist"
  "Caches:$APP_ID"
  "Caches:$LEGACY_ID"
  "WebKit:$APP_ID"
  "WebKit:$LEGACY_ID"
)

# ---- restore mode -----------------------------------------------------------
if [ "$DO_RESTORE" -eq 1 ]; then
  latest="$(ls -dt "$LIB"/meetily-newuser-backup-* 2>/dev/null | head -1 || true)"
  if [ -z "$latest" ] || [ ! -x "$latest/restore.sh" ]; then
    echo "No restorable backup found under $LIB/meetily-newuser-backup-*" >&2; exit 1
  fi
  echo "Restoring from: $latest"
  "$latest/restore.sh"
  exit 0
fi

# ---- stop any running instance ---------------------------------------------
echo "Stopping any running Meetily dev processes..."
pkill -f "target/debug/meetily"       2>/dev/null || true
pkill -f "target/release/meetily"     2>/dev/null || true
pkill -f "target/debug/memento"       2>/dev/null || true
pkill -f "target/release/memento"     2>/dev/null || true
pkill -f "next dev -p 3118"           2>/dev/null || true
pkill -f "tauri-auto.js"              2>/dev/null || true
pkill -f "cargo run --features"       2>/dev/null || true

# ---- back up + clear per-user state ----------------------------------------
TS="$(date +%Y%m%d-%H%M%S)"
BK="$LIB/meetily-newuser-backup-$TS"
mkdir -p "$BK/Application Support" "$BK/Preferences" "$BK/Caches" "$BK/WebKit"
echo "Backing up current state to: $BK"

for item in "${STATE_ITEMS[@]}"; do
  sub="${item%%:*}"; leaf="${item#*:}"
  src="$LIB/$sub/$leaf"
  if [ -e "$src" ]; then
    mv "$src" "$BK/$sub/"
    echo "  moved: $sub/$leaf"
  fi
done

# Optionally keep the (large) downloaded models: recreate the app-support dir
# and move just the models subfolder back into the fresh slate.
if [ "$KEEP_MODELS" -eq 1 ] && [ -d "$BK/Application Support/$APP_ID/models" ]; then
  mkdir -p "$LIB/Application Support/$APP_ID"
  mv "$BK/Application Support/$APP_ID/models" "$LIB/Application Support/$APP_ID/models"
  echo "  kept models (moved back into fresh state)"
fi

# ---- reset the managed-cloud gateway identity (keychain) --------------------
# Export values into the backup first so the reset stays restorable.
KC_DUMP="$BK/keychain-$KC_SERVICE.txt"
: > "$KC_DUMP"
for acct in device-id install-token; do
  if val="$(security find-generic-password -s "$KC_SERVICE" -a "$acct" -w 2>/dev/null)"; then
    printf '%s=%s\n' "$acct" "$val" >> "$KC_DUMP"
    security delete-generic-password -s "$KC_SERVICE" -a "$acct" >/dev/null 2>&1 || true
    echo "  reset keychain: $KC_SERVICE/$acct (backed up)"
  fi
done
[ -s "$KC_DUMP" ] || rm -f "$KC_DUMP"

# ---- optionally reset macOS TCC permissions ---------------------------------
if [ "$RESET_PERMS" -eq 1 ]; then
  echo "Resetting macOS TCC permissions for $APP_ID (app will re-prompt)..."
  for svc in Microphone ScreenCapture SystemPolicyAllFiles; do
    tccutil reset "$svc" "$APP_ID" >/dev/null 2>&1 || true
  done
fi

# ---- write a restore script into this backup --------------------------------
cat > "$BK/restore.sh" <<'EOF'
#!/bin/bash
# Restore original Meetily user state captured by run-as-new-user.sh.
set -e
BK="$(cd "$(dirname "$0")" && pwd)"
LIB="$HOME/Library"
# Discard any fresh state created since the reset, then move originals back.
for item in \
  "Application Support:com.meetily.ai" "Application Support:meetily" \
  "Preferences:com.meetily.ai.plist"  "Preferences:meetily.plist" \
  "Caches:com.meetily.ai"             "Caches:meetily" \
  "WebKit:com.meetily.ai"             "WebKit:meetily"; do
  sub="${item%%:*}"; leaf="${item#*:}"
  rm -rf "$LIB/$sub/$leaf"
  [ -e "$BK/$sub/$leaf" ] && mv "$BK/$sub/$leaf" "$LIB/$sub/"
done
# Restore keychain gateway identity if it was captured.
kc="$BK/keychain-meetily.gateway.txt"
if [ -f "$kc" ]; then
  while IFS='=' read -r acct val; do
    [ -n "$acct" ] || continue
    security add-generic-password -U -s "meetily.gateway" -a "$acct" -w "$val" >/dev/null 2>&1 || true
  done < "$kc"
fi
echo "Restored original Meetily state from $BK"
EOF
chmod +x "$BK/restore.sh"

echo "Clean new-user state ready. Restore anytime with:"
echo "  $BK/restore.sh   (or: $SCRIPT_DIR/$(basename "$0") --restore)"

# ---- launch the dev build ---------------------------------------------------
if [ "$DO_RUN" -eq 0 ]; then
  echo "--no-run: state reset only, not launching."
  exit 0
fi

cd "$SCRIPT_DIR"

# Bypass any configured HTTP proxy for localhost (Next HMR, Ollama on :11434, etc.)
export no_proxy="localhost,127.0.0.1,::1${no_proxy:+,$no_proxy}"
export NO_PROXY="localhost,127.0.0.1,::1${NO_PROXY:+,$NO_PROXY}"
export RUST_LOG="$LOG_LEVEL"

# Managed cloud AI (DeepSeek + SaluteSpeech transcription/speaker-detection) reaches the
# Memento gateway using MEMENTO_REGISTRATION_KEY. Release builds bake it in at CI build
# time; a local dev run must export it at runtime, or the gateway can't register a fresh
# device and recording fails with "Транскрипция недоступна / SaluteSpeech unavailable".
if [ -z "${MEMENTO_REGISTRATION_KEY:-}" ]; then
  echo "⚠️  MEMENTO_REGISTRATION_KEY is not set — managed SaluteSpeech (transcription +"
  echo "    speaker detection) will be UNAVAILABLE for this new user. Export it first:"
  echo "      export MEMENTO_REGISTRATION_KEY=<key>   # then re-run this script"
  echo "    (Or set a SaluteSpeech Authorization Key via Settings → Transcription as BYOK.)"
else
  echo "✅ MEMENTO_REGISTRATION_KEY present — managed SaluteSpeech enabled."
fi

# Install deps only if missing (this script resets user data, not the app build).
if [ ! -d node_modules ]; then
  echo "node_modules missing — installing dependencies..."
  pnpm install
fi

echo "Launching Meetily dev build as a new user (RUST_LOG=$LOG_LEVEL)..."
exec pnpm run tauri:dev
