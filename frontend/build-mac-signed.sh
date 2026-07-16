#!/usr/bin/env bash
# Build + sign + notarize + staple the meetily Tauri app for macOS arm64.
#
# Produces a Developer ID-signed, Apple-notarized .app and .dmg under
#   <repo>/target/release/bundle/{macos,dmg}/
#
# Credentials are read from the environment, or from a gitignored
# frontend/.env.signing (copy frontend/.env.signing.example and fill it in).
# Nothing secret is hardcoded here.
#
# Why the throwaway keychain: codesign can't drive a login-keychain Developer ID
# key from a non-interactive / headless shell (fails with errSecInternalComponent).
# Importing the .p12 into a temp keychain with an explicit codesign partition list
# signs reliably and needs no login password.
#
# Usage:
#   frontend/build-mac-signed.sh                 # full signed + notarized build
#   frontend/build-mac-signed.sh --skip-dmg-notarize   # sign+notarize app, skip DMG notarize
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FRONTEND="$SCRIPT_DIR"
REPO_ROOT="$(cd "$FRONTEND/.." && pwd)"
CONF="$FRONTEND/src-tauri/tauri.conf.json"
BAK="$CONF.signbuild.bak"
KC="${SIGNING_KEYCHAIN:-/tmp/meetily-sign.keychain-db}"
KCPW="meetily-temp-kc-pw"

skip_dmg_notarize=0
[[ "${1:-}" == "--skip-dmg-notarize" ]] && skip_dmg_notarize=1

# ---------- load credentials ----------
if [[ -f "$FRONTEND/.env.signing" ]]; then
  echo "==> Loading credentials from frontend/.env.signing"
  set -a; # shellcheck disable=SC1091
  source "$FRONTEND/.env.signing"; set +a
fi

: "${APPLE_ID:?set APPLE_ID (Apple account email) in env or frontend/.env.signing}"
: "${APPLE_PASSWORD:?set APPLE_PASSWORD (app-specific password) — Tauri/notarytool env name}"
: "${APPLE_TEAM_ID:?set APPLE_TEAM_ID}"
: "${SIGNING_P12:?set SIGNING_P12 (path to the Developer ID .p12)}"
: "${SIGNING_P12_PASSWORD:?set SIGNING_P12_PASSWORD (the .p12 password)}"
: "${APPLE_SIGNING_IDENTITY:?set APPLE_SIGNING_IDENTITY (e.g. 'Developer ID Application: Name (TEAMID)')}"

[[ -f "$SIGNING_P12" ]] || { echo "error: SIGNING_P12 not found: $SIGNING_P12" >&2; exit 1; }

# Tauri picks these up for notarization; keep the identity for signing.
export APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID APPLE_SIGNING_IDENTITY
# NOTE: do NOT export APPLE_CERTIFICATE — we manage the keychain below so the
# imported key gets a codesign-usable partition list.

# ---------- managed cloud AI (Memento gateway) ----------
# MEMENTO_REGISTRATION_KEY is embedded into the binary at Rust compile time via
# option_env!("MEMENTO_REGISTRATION_KEY") (see src-tauri/src/gateway_identity.rs), so the
# shipped app reaches managed DeepSeek + SaluteSpeech with no per-user API key. It's a
# gateway registration proof, not a provider master credential. Export it before the build
# (from the environment or frontend/.env.signing). Not required to build — without it the
# release simply omits managed AI and users configure their own keys.
if [[ -n "${MEMENTO_REGISTRATION_KEY:-}" ]]; then
  export MEMENTO_REGISTRATION_KEY
  echo "==> MEMENTO_REGISTRATION_KEY present — embedding managed DeepSeek + SaluteSpeech."
else
  echo "WARNING: MEMENTO_REGISTRATION_KEY is not set — this signed build will NOT include" >&2
  echo "         managed cloud AI (DeepSeek/SaluteSpeech unavailable to users). Set it in the" >&2
  echo "         environment or frontend/.env.signing to embed it." >&2
fi

cd "$FRONTEND"

ORIG_KEYCHAINS="$(security list-keychains -d user | sed -e 's/^[[:space:]]*//' -e 's/"//g')"

cleanup() {
  local st=$?
  [[ -f "$BAK" ]] && mv -f "$BAK" "$CONF"
  security list-keychains -d user -s $ORIG_KEYCHAINS >/dev/null 2>&1 || true
  security delete-keychain "$KC" >/dev/null 2>&1 || true
  return $st
}
trap cleanup EXIT

# ---------- throwaway signing keychain from the .p12 ----------
echo "==> Preparing signing keychain from $SIGNING_P12"
security delete-keychain "$KC" 2>/dev/null || true
security create-keychain -p "$KCPW" "$KC"
security set-keychain-settings "$KC"                    # disable auto-lock timeout
security unlock-keychain -p "$KCPW" "$KC"
security import "$SIGNING_P12" -k "$KC" -P "$SIGNING_P12_PASSWORD" -T /usr/bin/codesign -A
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KCPW" "$KC" >/dev/null
security list-keychains -d user -s "$KC" $ORIG_KEYCHAINS   # prepend
if ! security find-identity -v -p codesigning "$KC" | grep -q "$APPLE_SIGNING_IDENTITY"; then
  echo "error: identity '$APPLE_SIGNING_IDENTITY' not found in the imported .p12" >&2
  security find-identity -v -p codesigning "$KC" >&2
  exit 1
fi

# ---------- patch config: real identity + no updater artifacts ----------
# tauri.conf.json ships signingIdentity "-" (ad-hoc) and createUpdaterArtifacts true.
# We can't sign updater artifacts (no private key for the configured pubkey), and we
# need the real Developer ID identity. Patched here, restored by the EXIT trap.
cp -f "$CONF" "$BAK"
APPLE_SIGNING_IDENTITY="$APPLE_SIGNING_IDENTITY" python3 - "$CONF" <<'PY'
import json, os, sys
p = sys.argv[1]
c = json.load(open(p))
c["bundle"]["macOS"]["signingIdentity"] = os.environ["APPLE_SIGNING_IDENTITY"]
c["bundle"]["createUpdaterArtifacts"] = False
json.dump(c, open(p, "w"), indent=4)
print("patched tauri.conf.json: signingIdentity + createUpdaterArtifacts=false")
PY

# ---------- build (GPU auto-detected: coreml on Apple Silicon) ----------
echo "==> tauri build (sign app + sidecars, notarize + staple .app)"
date
corepack pnpm run tauri:build
status=$?
if [[ $status -ne 0 ]]; then
  echo "==> tauri build failed (status $status)" >&2
  exit $status
fi

# ---------- notarize + staple the DMG (Tauri only does the .app) ----------
DMG="$(ls -t "$REPO_ROOT"/target/release/bundle/dmg/*.dmg 2>/dev/null | head -1)"
APP="$(ls -dt "$REPO_ROOT"/target/release/bundle/macos/*.app 2>/dev/null | head -1)"

if [[ -n "$DMG" && $skip_dmg_notarize -eq 0 ]]; then
  echo "==> Notarizing DMG: $DMG"
  xcrun notarytool submit "$DMG" \
    --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_PASSWORD" --wait
  xcrun stapler staple "$DMG"
fi

# ---------- verify ----------
echo ""; echo "==> Verification"
if [[ -n "$APP" ]]; then
  echo "--- app: $APP"
  codesign --verify --deep --strict --verbose=2 "$APP" 2>&1 | tail -2
  spctl -a -t exec -vv "$APP" 2>&1
  xcrun stapler validate "$APP" 2>&1 | tail -1
fi
if [[ -n "$DMG" ]]; then
  echo "--- dmg: $DMG"
  spctl -a -t open --context context:primary-signature -vv "$DMG" 2>&1
  xcrun stapler validate "$DMG" 2>&1 | tail -1
  echo ""; echo "==> Artifact:"; ls -lh "$DMG"; shasum -a 256 "$DMG"
fi
echo "==> Done."
