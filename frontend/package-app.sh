#!/bin/bash

# Exit on error
set -e

echo "Cleaning up previous builds..."
rm -rf .next
rm -rf out
rm -rf src-tauri/target/release

echo "Installing dependencies..."
pnpm install

echo "Building Next.js app..."
pnpm build

echo "Building Tauri app..."
pnpm tauri build

echo "App packaging complete! Check src-tauri/target/release/bundle for the packaged app."