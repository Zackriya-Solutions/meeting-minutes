#!/usr/bin/env bash
# Build + sign + notarize + staple the meetily Tauri app for macOS x86_64 (Intel).
#
# This CROSS-COMPILES to x86_64-apple-darwin — run it on any Mac (incl. Apple
# Silicon) and it emits an Intel .app + .dmg under
#   <repo>/target/x86_64-apple-darwin/release/bundle/{macos,dmg}/
#
# It mirrors build-mac-signed.sh (arm64) with the Intel-specific bits added:
#   * --target x86_64-apple-darwin passed to `tauri build`
#   * Metal GPU feature (Intel Macs have no Neural Engine, so no CoreML)
#   * x86_64 Rust std compiled from source via -Z build-std (see below)
#   * x86_64 sidecars produced/verified before bundling (llama-helper + ffmpeg)
#   * bundle paths under target/<triple>/release/bundle
#
# Why -Z build-std instead of a downloaded/rustup std:
#   This repo's toolchain is Homebrew's rust (no rustup), which ships only the
#   host std. A downloaded OFFICIAL x86_64 std will NOT load — Homebrew's rustc
#   reports its version as "1.94.0 (<hash> <date>) (Homebrew)" while the official
#   std was built by "...(<hash> <date>)" (no suffix), so rustc rejects it with
#   E0514 "incompatible version of rustc". RUSTC_OVERRIDE_VERSION_STRING does not
#   fix the metadata check. Instead we compile std from the bundled `rust-src`
#   with the SAME compiler (RUSTC_BOOTSTRAP=1 lets stable accept -Z build-std),
#   so the version matches exactly. No rustup, no network for the std.
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
#   frontend/build-mac-x86-signed.sh                       # full signed + notarized build
#   frontend/build-mac-x86-signed.sh --skip-dmg-notarize   # sign+notarize app, skip DMG notarize
#   frontend/build-mac-x86-signed.sh --fetch-ffmpeg        # download an x86_64 ffmpeg if missing
set -uo pipefail

TARGET="x86_64-apple-darwin"
GPU_FEATURE="metal"   # Intel Macs: Metal GPU. CoreML (Neural Engine) is Apple-Silicon only.

# Compile std from source with the current (Homebrew) compiler — see header.
export RUSTC_BOOTSTRAP=1
BUILD_STD=(-Z build-std=std,panic_abort)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FRONTEND="$SCRIPT_DIR"
REPO_ROOT="$(cd "$FRONTEND/.." && pwd)"
CONF="$FRONTEND/src-tauri/tauri.conf.json"
BAK="$CONF.signbuild.bak"
BINARIES_DIR="$FRONTEND/src-tauri/binaries"
LLAMA_HELPER_DIR="$REPO_ROOT/llama-helper"
KC="${SIGNING_KEYCHAIN:-/tmp/meetily-sign.keychain-db}"
KCPW="meetily-temp-kc-pw"

skip_dmg_notarize=0
fetch_ffmpeg=0
for arg in "$@"; do
  case "$arg" in
    --skip-dmg-notarize) skip_dmg_notarize=1 ;;
    --fetch-ffmpeg)      fetch_ffmpeg=1 ;;
    *) echo "error: unknown argument: $arg" >&2; exit 2 ;;
  esac
done

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

# ---------- preflight: rust-src (needed to build std from source) ----------
SYSROOT="$(rustc --print sysroot)"
STD_SRC="$SYSROOT/lib/rustlib/src/rust/library/std/Cargo.toml"
if [[ ! -f "$STD_SRC" ]]; then
  cat >&2 <<EOF
error: the 'rust-src' component is required to build std for $TARGET.
       Not found under: $SYSROOT/lib/rustlib/src
       With Homebrew's rust it normally ships with the formula; reinstall with:
         brew reinstall rust
       (A rustup toolchain would instead use: rustup component add rust-src)
EOF
  exit 1
fi
echo "==> Using build-std (rust-src at $SYSROOT/lib/rustlib/src)"

# ---------- preflight: x86_64 llama-helper sidecar ----------
# llama-helper is a workspace member; cross-build it for x86_64 (Metal only —
# llama-cpp-2 has no CoreML feature) and drop it in next to the arm64 sidecar.
echo "==> Building llama-helper sidecar for $TARGET (build-std, --features metal)"
( cd "$LLAMA_HELPER_DIR" && cargo build --release --target "$TARGET" "${BUILD_STD[@]}" --features metal ) \
  || { echo "error: llama-helper cross-build for $TARGET failed" >&2; exit 1; }

LLAMA_SRC="$REPO_ROOT/target/$TARGET/release/llama-helper"
LLAMA_SIDECAR="$BINARIES_DIR/llama-helper-$TARGET"
[[ -f "$LLAMA_SRC" ]] || { echo "error: llama-helper binary not found at $LLAMA_SRC" >&2; exit 1; }
mkdir -p "$BINARIES_DIR"
cp -f "$LLAMA_SRC" "$LLAMA_SIDECAR"
echo "    -> $LLAMA_SIDECAR"

# ---------- preflight: x86_64 ffmpeg sidecar ----------
# Tauri looks for binaries/ffmpeg-<triple>. We ship the arm64 one; make sure a
# FULL x86_64 one exists (meetily encodes recordings as AAC/MP4, so a stripped
# audio-only ffmpeg is not enough). --fetch-ffmpeg pulls the osxexperts Intel
# build — the x86_64 counterpart of the arm64 sidecar.
FFMPEG_SIDECAR="$BINARIES_DIR/ffmpeg-$TARGET"
ffmpeg_is_x86() { lipo -archs "$1" 2>/dev/null | grep -qw x86_64; }

if [[ -f "$FFMPEG_SIDECAR" ]] && ffmpeg_is_x86 "$FFMPEG_SIDECAR"; then
  echo "==> ffmpeg sidecar present: $FFMPEG_SIDECAR ($(du -h "$FFMPEG_SIDECAR" | cut -f1))"
else
  echo "==> Need an x86_64 ffmpeg sidecar: $FFMPEG_SIDECAR"
  produced=0
  # 1) Thin a universal ffmpeg we already have (checked-in arm64 file or PATH).
  for cand in "$BINARIES_DIR/ffmpeg-aarch64-apple-darwin" "$(command -v ffmpeg 2>/dev/null || true)"; do
    [[ -n "$cand" && -f "$cand" ]] || continue
    if lipo -archs "$cand" 2>/dev/null | grep -qw x86_64; then
      echo "    thinning x86_64 slice from universal binary: $cand"
      lipo -thin x86_64 "$cand" -output "$FFMPEG_SIDECAR" && produced=1 && break
    fi
  done
  # 2) Optional network fetch of a full static Intel build (osxexperts, v8.0).
  if [[ $produced -eq 0 && $fetch_ffmpeg -eq 1 ]]; then
    URL="https://www.osxexperts.net/ffmpeg80intel.zip"
    echo "    fetching full x86_64 ffmpeg from $URL"
    tmp="$(mktemp -d)"
    if curl -fsSL "$URL" -o "$tmp/ffmpeg.zip" && unzip -qo "$tmp/ffmpeg.zip" -d "$tmp"; then
      got="$(find "$tmp" -type f -name ffmpeg -not -path '*__MACOSX*' | head -1)"
      if [[ -n "$got" ]] && ffmpeg_is_x86 "$got"; then
        cp -f "$got" "$FFMPEG_SIDECAR" && produced=1
      else
        echo "error: downloaded ffmpeg is not an x86_64 binary" >&2
      fi
    fi
    rm -rf "$tmp"
  fi
  if [[ $produced -eq 0 ]]; then
    cat >&2 <<EOF
error: no x86_64 ffmpeg available for the sidecar.
       Provide one of:
         * drop a FULL x86_64 ffmpeg (with AAC encoder + MP4 muxer) at:
             $FFMPEG_SIDECAR
         * or re-run with --fetch-ffmpeg to download the osxexperts Intel build.
EOF
    exit 1
  fi
  xattr -c "$FFMPEG_SIDECAR" 2>/dev/null || true
  chmod +x "$FFMPEG_SIDECAR"
  echo "    -> $FFMPEG_SIDECAR"
fi
ffmpeg_is_x86 "$FFMPEG_SIDECAR" || { echo "error: $FFMPEG_SIDECAR is not x86_64" >&2; exit 1; }

# ---------- signing keychain + config patch (restored on exit) ----------
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

# ---------- build (cross-compile to x86_64, build-std, Metal GPU) ----------
echo "==> tauri build --target $TARGET (build-std, --features $GPU_FEATURE); sign app + sidecars, notarize + staple .app"
date
# Call the tauri CLI directly (pnpm is on PATH for the beforeBuildCommand) so the
# cargo args after `--` reach cargo unambiguously.
"$FRONTEND/node_modules/.bin/tauri" build --target "$TARGET" -- "${BUILD_STD[@]}" --features "$GPU_FEATURE"
status=$?
if [[ $status -ne 0 ]]; then
  echo "==> tauri build failed (status $status)" >&2
  exit $status
fi

# ---------- notarize + staple the DMG (Tauri only does the .app) ----------
BUNDLE_DIR="$REPO_ROOT/target/$TARGET/release/bundle"
DMG="$(ls -t "$BUNDLE_DIR"/dmg/*.dmg 2>/dev/null | head -1)"
APP="$(ls -dt "$BUNDLE_DIR"/macos/*.app 2>/dev/null | head -1)"

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
  file "$APP/Contents/MacOS/"* 2>/dev/null | head -1
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
