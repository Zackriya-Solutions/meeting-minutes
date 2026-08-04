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

# Pre-warm the Next.js dev server so all chunks are compiled before
# Tauri opens the webview. Once warm, kill it — tauri dev restarts it
# instantly from the .next cache (no ChunkLoadError).
echo "Pre-warming Next.js dev server..."
pnpm dev &
DEV_WARM_PID=$!
echo "Waiting for Next.js to finish compiling..."
pnpm exec wait-on "http://localhost:3118/_next/static/chunks/main-app.js" \
     "http://localhost:3118/_next/static/chunks/app/layout.js" \
     --timeout 120000 2>/dev/null \
  || pnpm exec wait-on "http://localhost:3118" --timeout 120000
echo "Next.js ready — handing off to Tauri dev..."
kill $DEV_WARM_PID 2>/dev/null || true
wait $DEV_WARM_PID 2>/dev/null || true

echo "Building Tauri app..."
pnpm run tauri dev &
TAURI_PID=$!
echo $TAURI_PID > /tmp/meetily.pid
echo "Meetily PID: $TAURI_PID (saved to /tmp/meetily.pid)"
echo "  Kill with: kill \$(cat /tmp/meetily.pid)"
wait $TAURI_PID
