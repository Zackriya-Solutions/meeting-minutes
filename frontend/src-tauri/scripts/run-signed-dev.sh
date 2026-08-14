#!/usr/bin/env bash

# Cargo runner for macOS development builds. Rust's default linker signature is
# ad-hoc and its designated requirement is only the binary's current CDHash.
# Rebuilding therefore makes Keychain treat Memento as a different application.
# Sign the freshly built executable with one stable local identity before launch
# so a single "Always Allow" grant survives later dev rebuilds.

set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "Usage: run-signed-dev.sh <executable> [arguments...]" >&2
    exit 2
fi

executable="$1"
shift

signing_identity="${MEMENTO_DEV_SIGNING_IDENTITY:-}"
if [[ -z "$signing_identity" ]]; then
    echo "MEMENTO_DEV_SIGNING_IDENTITY is missing; refusing to launch an unsigned dev build." >&2
    exit 1
fi

/usr/bin/codesign \
    --force \
    --sign "$signing_identity" \
    --identifier com.meetily.ai \
    --timestamp=none \
    "$executable"

/usr/bin/codesign --verify --strict "$executable"
exec "$executable" "$@"
