#!/usr/bin/env bash
# Builds a self-contained CUDA-enabled AppImage.
#
# linuxdeploy's automatic dependency scan only follows the meetily binary's actual
# ELF NEEDED chain — it never discovers onnxruntime's CUDA execution provider
# (libonnxruntime_providers_cuda.so, loaded via dlopen at runtime, not a link-time
# dependency) or that provider's own CUDA 12 dependencies (linked against CUDA 12,
# not whatever CUDA toolkit is installed on this machine). So a plain `tauri build`
# produces a working AppImage that silently falls back to CPU for Parakeet. This
# script patches the constructed AppDir with both sets of libraries and repackages,
# so the result actually offloads to the GPU with nothing extra needed at launch.
#
# Requires the CUDA 12 compat libs already downloaded via:
#   pip install --target ~/.local/share/meetily/cuda12-compat --no-deps \
#     nvidia-cuda-runtime-cu12 nvidia-cublas-cu12 nvidia-cufft-cu12
# and linuxdeploy already cached by a prior `tauri build` (~/.cache/tauri/linuxdeploy-x86_64.AppImage).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FRONTEND_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET_RELEASE_DIR="$FRONTEND_DIR/../target/release"
APPDIR="$TARGET_RELEASE_DIR/bundle/appimage/meetily.AppDir"
CUDA12_COMPAT_DIR="$HOME/.local/share/meetily/cuda12-compat"
LINUXDEPLOY="$HOME/.cache/tauri/linuxdeploy-x86_64.AppImage"

if [ ! -d "$CUDA12_COMPAT_DIR" ]; then
    echo "error: $CUDA12_COMPAT_DIR not found." >&2
    echo "Run this first:" >&2
    echo "  pip install --target $CUDA12_COMPAT_DIR --no-deps nvidia-cuda-runtime-cu12 nvidia-cublas-cu12 nvidia-cufft-cu12" >&2
    exit 1
fi

cd "$FRONTEND_DIR"
cargo build --manifest-path src-tauri/Cargo.toml --release --features cuda

# Runs the normal pipeline (build -> construct AppDir -> package AppImage). The
# resulting AppImage is CUDA-incomplete (see comment above) — it gets replaced by
# the repackage step below, which reuses the same AppDir this step constructs.
# `|| true`: this repo's updater-signing step (unrelated, needs a release private
# key this machine doesn't have) makes the command exit non-zero even when
# bundling itself succeeded — checking for the AppDir below is the real signal.
NO_STRIP=1 pnpm exec tauri build -- --features cuda || true

if [ ! -d "$APPDIR" ]; then
    echo "error: AppDir not found at $APPDIR" >&2
    exit 1
fi

echo "Patching AppDir with the onnxruntime CUDA provider + CUDA 12 compat libs..."
# Only cuda + shared — NOT the also-downloaded tensorrt provider, which needs
# libnvinfer.so.10 (TensorRT) that we don't have and don't use. Remove it in case a
# prior run of this script left it behind (the AppDir persists across runs).
rm -f "$APPDIR/usr/lib/libonnxruntime_providers_tensorrt.so"
cp -v "$TARGET_RELEASE_DIR/libonnxruntime_providers_cuda.so" "$APPDIR/usr/lib/"
cp -v "$TARGET_RELEASE_DIR/libonnxruntime_providers_shared.so" "$APPDIR/usr/lib/"
find "$CUDA12_COMPAT_DIR" -iname "*.so*" -exec cp -v {} "$APPDIR/usr/lib/" \;

echo "Repackaging..."
cd "$TARGET_RELEASE_DIR/bundle/appimage"
# linuxdeploy independently re-resolves dependencies for newly-added files like
# libonnxruntime_providers_cuda.so via ldd against the normal library search path —
# it doesn't trust files already sitting in the AppDir. Point it at the compat libs
# so that resolution actually succeeds instead of erroring on libcublasLt.so.12 etc.
export LD_LIBRARY_PATH="$CUDA12_COMPAT_DIR/nvidia/cublas/lib:$CUDA12_COMPAT_DIR/nvidia/cuda_runtime/lib:$CUDA12_COMPAT_DIR/nvidia/cufft/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
NO_STRIP=1 "$LINUXDEPLOY" --appdir meetily.AppDir --output appimage

echo "Done: $TARGET_RELEASE_DIR/bundle/appimage/meetily-x86_64.AppImage"
