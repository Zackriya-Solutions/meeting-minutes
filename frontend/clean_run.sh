#!/bin/bash

# Exit on error
set -e

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

# Clean deps and generated output, but keep .next compilation cache
# to avoid a cold-start race where Tauri opens the window before
# Next.js finishes compiling its chunks.
echo "Cleaning up..."
rm -rf node_modules .pnp.cjs out

echo "Installing dependencies..."
pnpm install

# Start the Next.js dev server and keep it running throughout.
# Wait until it is fully compiled before opening Tauri, so the
# webview always finds all chunks ready (no ChunkLoadError).
echo "Starting Next.js dev server..."
pnpm dev &
DEV_PID=$!

echo "Waiting for Next.js to be ready (http://localhost:3118)..."
pnpm exec wait-on "http://localhost:3118" --timeout 120000
echo "Next.js ready."

# Run Tauri with beforeDevCommand disabled so it reuses the server above.
echo "Building Tauri app..."
TAURI_SKIP_DEVSERVER_CHECK=true \
  pnpm exec tauri dev --no-watch -- --features platform-default &
TAURI_PID=$!
echo $TAURI_PID > /tmp/meetily.pid
echo "Meetily PID: $TAURI_PID (saved to /tmp/meetily.pid)"
echo "  Kill with: kill \$(cat /tmp/meetily.pid)"

# Wait for either process to exit; clean up both on exit
trap "kill $DEV_PID $TAURI_PID 2>/dev/null" EXIT
wait $TAURI_PID
