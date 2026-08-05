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
DEVELOPER_ID_G2_CA="${SIGNING_INTERMEDIATE:-/tmp/apple-developer-id-g2.der}"

skip_dmg_notarize=0
[[ "${1:-}" == "--skip-dmg-notarize" ]] && skip_dmg_notarize=1

# ---------- load credentials ----------
if [[ -f "$FRONTEND/.env.signing" ]]; then
  echo "==> Loading credentials from frontend/.env.signing"
  set -a; # shellcheck disable=SC1091
  source "$FRONTEND/.env.signing"; set +a
fi

# Keep the local certificate next to this script without baking an absolute
# developer-machine path into the ignored credentials file.
SIGNING_P12="${SIGNING_P12:-Certificates.p12}"
if [[ "$SIGNING_P12" != /* ]]; then
  SIGNING_P12="$FRONTEND/$SIGNING_P12"
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
  rm -f "$DEVELOPER_ID_G2_CA"
  return $st
}
trap cleanup EXIT

# ---------- throwaway signing keychain from the .p12 ----------
echo "==> Preparing signing keychain from $SIGNING_P12"
security delete-keychain "$KC" 2>/dev/null || true
security create-keychain -p "$KCPW" "$KC"
security set-keychain-settings "$KC"                    # disable auto-lock timeout
security unlock-keychain -p "$KCPW" "$KC"
# New Developer ID certificates use the G2 intermediate. Some macOS installations
# can fetch it for certificate verification but codesign will not fetch it while
# resolving an identity in an isolated keychain, which results in
# "unable to build chain ... errSecInternalComponent".
curl -fsSL https://www.apple.com/certificateauthority/DeveloperIDG2CA.cer -o "$DEVELOPER_ID_G2_CA"
security import "$DEVELOPER_ID_G2_CA" -k "$KC" -A >/dev/null
security import "$SIGNING_P12" -k "$KC" -P "$SIGNING_P12_PASSWORD" -T /usr/bin/codesign -A
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KCPW" "$KC" >/dev/null
security list-keychains -d user -s "$KC" $ORIG_KEYCHAINS   # prepend
if ! security find-identity -v -p codesigning | grep -Fq "$APPLE_SIGNING_IDENTITY"; then
  echo "error: identity '$APPLE_SIGNING_IDENTITY' not found in the imported .p12" >&2
  security find-identity -v -p codesigning >&2
  exit 1
fi

# ---------- updater signing key ----------
# createUpdaterArtifacts needs the minisign private key matching
# plugins.updater.pubkey in tauri.conf.json. Without it the bundler aborts, so we
# turn the artifacts off and still produce an installable (but unpublishable) DMG.
# The CLI wants the key *content*, not a path — a path in TAURI_SIGNING_PRIVATE_KEY
# fails with "failed to decode base64 secret key".
UPDATER_KEY="${TAURI_SIGNING_PRIVATE_KEY_PATH:-$HOME/.memento/updater/memento-updater.key}"
updater_artifacts=false
if [[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
  updater_artifacts=true
elif [[ -f "$UPDATER_KEY" ]]; then
  TAURI_SIGNING_PRIVATE_KEY="$(cat "$UPDATER_KEY")"
  updater_artifacts=true
fi
if [[ "$updater_artifacts" == true ]]; then
  # Must be exported even when empty: with the var unset the CLI tries to read the
  # password from the tty and dies ("Device not configured") in a headless shell.
  export TAURI_SIGNING_PRIVATE_KEY
  export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"
  echo "==> Updater key found — emitting signed .app.tar.gz for auto-update."
else
  echo "WARNING: no updater signing key (checked TAURI_SIGNING_PRIVATE_KEY and $UPDATER_KEY)." >&2
  echo "         Building WITHOUT updater artifacts — this build cannot be published to the" >&2
  echo "         auto-update channel. See docs/AUTOUPDATE.md." >&2
fi

# ---------- patch config: real identity + updater artifacts ----------
# tauri.conf.json ships signingIdentity "-" (ad-hoc); we need the real Developer ID
# identity, and updater artifacts only when we hold the signing key. Patched here,
# restored by the EXIT trap.
cp -f "$CONF" "$BAK"
APPLE_SIGNING_IDENTITY="$APPLE_SIGNING_IDENTITY" UPDATER_ARTIFACTS="$updater_artifacts" python3 - "$CONF" <<'PY'
import json, os, sys
p = sys.argv[1]
c = json.load(open(p))
c["bundle"]["macOS"]["signingIdentity"] = os.environ["APPLE_SIGNING_IDENTITY"]
updater = os.environ["UPDATER_ARTIFACTS"] == "true"
c["bundle"]["createUpdaterArtifacts"] = updater
json.dump(c, open(p, "w"), indent=4)
print(f"patched tauri.conf.json: signingIdentity + createUpdaterArtifacts={str(updater).lower()}")
PY

# ---------- build (GPU auto-detected: coreml on Apple Silicon) ----------
echo "==> tauri build (sign app + sidecars, notarize + staple .app)"
date
if command -v corepack >/dev/null 2>&1; then
  corepack pnpm run tauri:build
elif command -v pnpm >/dev/null 2>&1; then
  pnpm run tauri:build
else
  echo "error: neither corepack nor pnpm is available in PATH" >&2
  exit 127
fi
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
if [[ "$updater_artifacts" == true ]]; then
  TARGZ="$(ls -t "$REPO_ROOT"/target/release/bundle/macos/*.app.tar.gz 2>/dev/null | head -1)"
  if [[ -n "$TARGZ" && -f "$TARGZ.sig" ]]; then
    echo ""; echo "==> Updater artifact:"; ls -lh "$TARGZ" "$TARGZ.sig"
    echo "    Publish it with: scripts/publish-update-obs.py"
  else
    echo "WARNING: updater artifacts were requested but no signed .app.tar.gz was produced." >&2
  fi
fi
echo "==> Done."
