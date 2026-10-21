# CI/CD Hardware Acceleration Guide

This document explains the hardware acceleration and CPU-portability configuration used by Meetily CI/CD workflows.

## Overview

CI chooses acceleration by platform while keeping Windows release assets portable across the supported CPU baseline:

| Platform | Acceleration | Technology | Distribution baseline |
| --- | --- | --- | --- |
| **macOS** | GPU | Metal by default; CoreML available on Apple Silicon | Apple Silicon build target |
| **Windows** | GPU | Vulkan | AVX2-capable x64 CPU; AVX-512 disabled |
| **Linux** | CPU optimized in CI | OpenBLAS | Source-build configuration |

Windows release packages use Vulkan-enabled Whisper. They do not automatically select CUDA. CUDA requires NVIDIA hardware, the CUDA toolkit, and an appropriately configured source build. The Vulkan SDK is a native-build prerequisite and is separate from frontend dependency installation with pnpm.

## Windows Package Configuration (Enabled)

### 1. Windows Builds (Vulkan GPU)

```yaml
args: --target x86_64-pc-windows-msvc --features vulkan
```

**What this provides:**

- Vulkan-enabled Whisper for Windows packages.
- Compatibility with Vulkan-capable AMD, Intel, and NVIDIA hardware.
- A repeatable CI configuration instead of a package specialized for the CI runner.

**How it works:**

- CI installs Vulkan SDK `1.4.309.0` and validates its environment before building.
- The Tauri build enables the Vulkan feature for Windows.
- The packaged artifact remains Vulkan-based; CUDA is a source-build choice.

### 2. Windows CPU Portability

A release installer is intentionally different from a build optimized for one local machine:

- Rust targets `x86-64-v2`.
- Native Whisper retains AVX2 with host-native specialization disabled.
- `GGML_AVX512`, `GGML_AVX512_VBMI`, `GGML_AVX512_VNNI`, and `GGML_AVX512_BF16` must all be OFF.

The workflows set `CMAKE_PROJECT_INCLUDE` to `force-portable-ggml.cmake`, which forces:

```cmake
set(GGML_NATIVE OFF CACHE BOOL "Build without host CPU specialization" FORCE)
```

Rust target flags do not configure Whisper's native C/C++ compilation. Do not replace the checked-in release configuration with the old `WHISPER_NATIVE=OFF` workaround or plain `GGML_*` environment variables.

### 3. Portable Cache and Pre-Bundle Verification

Windows portable builds use the `windows-portable-v1` Rust-cache prefix, preventing an earlier host-native target cache from being restored through a fallback key.

Before bundling, `verify-portable-ggml.cjs` reads Whisper's generated CMake cache. It fails the build if native-build evidence is missing, `GGML_NATIVE` is enabled, or any of the four AVX-512 options is enabled. This prevents an unverified native library from becoming an installer asset.

## Linux Builds (OpenBLAS CPU)

```yaml
args: --target x86_64-unknown-linux-gnu --features openblas
```

**Why OpenBLAS in CI:**

- GitHub Actions Linux runners do not provide a GPU for release-style acceleration tests.
- OpenBLAS provides optimized CPU operations without a GPU dependency.
- Linux installation remains source-built, so local acceleration should match the configured source environment.

Linux source builds can use a CUDA, Vulkan, ROCm, OpenBLAS, or CPU configuration when the required hardware and toolchain are available. See the source-build guide for local setup.

## macOS Builds (Metal GPU)

Metal is enabled by default for macOS builds. Apple Silicon builds can also use CoreML. No separate feature flag is required for the standard macOS package build.

## Updated Workflows

### 1. `build.yml` (Shared Release Workflow)

When building Windows, the shared workflow:

- Enables the Vulkan feature.
- Installs and validates Vulkan SDK `1.4.309.0`.
- Applies the portable CMake hook and `x86-64-v2` Rust target.
- Uses the `windows-portable-v1` cache prefix.
- Runs the pre-bundle Whisper verification.

For Linux builds, it can enable OpenBLAS. macOS uses Metal by default.

### 2. `build-devtest.yml` (Windows DevTest)

The Windows DevTest path applies the same portable CMake hook, Rust target, cache prefix, Vulkan setup, and pre-bundle verification as a release build. Keep those safeguards in place when changing DevTest behavior.

### 3. `build-windows.yml` (Standalone Windows)

The standalone Windows workflow uses the Vulkan feature and applies the same portability safeguards before packaging. A change is incomplete if it updates only the shared workflow or only this standalone workflow.

## Verification

### How to Verify a Windows Package Build

1. **Check the workflow configuration**
   - Windows build arguments include `--features vulkan`.
   - Vulkan SDK `1.4.309.0` is installed and validated.
   - `CMAKE_PROJECT_INCLUDE` uses the portable hook.
   - Rust uses `-C target-cpu=x86-64-v2`.

2. **Check native Whisper configuration**
   - `GGML_NATIVE` is OFF.
   - All four AVX-512 options are OFF.
   - The `verify-portable-ggml.cjs` command completes before bundling.

3. **Check cache isolation**
   - Windows builds use the `windows-portable-v1` cache prefix.
   - Do not reuse a native cache whose CPU configuration is unknown.

### Runtime Verification

Confirm the packaged application runs on an AVX2-capable Windows computer without AVX-512. Treat non-AVX2 coverage as separate from this Whisper package baseline.

## Technical Details

### Whisper Backends

The Whisper integration supports these acceleration paths:

```toml
metal = ["whisper-rs/metal"]
cuda = ["whisper-rs/cuda"]
vulkan = ["whisper-rs/vulkan"]
hipblas = ["whisper-rs/hipblas"]
openblas = ["whisper-rs/openblas"]
```

### Why Windows CI Uses Vulkan Instead of CUDA

CUDA needs compatible NVIDIA hardware, drivers, and the CUDA toolkit. The standard Windows package instead uses Vulkan so CI can produce one configured artifact without selecting CUDA for end users. Contributors who need CUDA should create a compatible source build.

### Linux Source Builds

OpenBLAS is appropriate for Linux CI runners without a GPU. For local Linux development, choose the backend that matches the installed SDK and hardware rather than assuming the Windows package configuration applies.

## Troubleshooting

### Build Fails with a Windows Vulkan Error

- Confirm the Vulkan SDK installation and environment-validation steps completed.
- Confirm the workflow still uses SDK `1.4.309.0`.
- pnpm installation does not install or repair the Vulkan SDK.

### Portability Verification Fails

Treat the failure as a packaging blocker. Inspect the generated Whisper CMake cache and the portable cache prefix; do not bypass the pre-bundle verifier. A fresh portable build is safer than reusing a cache with unknown CPU flags.

### CUDA Is Needed

Use a compatible NVIDIA source-build environment with the CUDA toolkit. The standard Windows installer intentionally remains the Vulkan build.

## Related Documentation

- [Building from Source](../../docs/BUILDING.md)
- [Workflow Overview](WORKFLOWS_OVERVIEW.md)
- [DevTest Workflow Guide](README_DEVTEST.md)
- [Whisper.cpp](https://github.com/ggerganov/whisper.cpp)

## Summary

- Windows packages use Vulkan-enabled Whisper, retain AVX2, and disable AVX-512.
- The portable CMake hook, Rust target, cache prefix, Vulkan setup, and pre-bundle verification work together; preserve them as a unit.
- CUDA remains an appropriately configured source-build option.
- Linux remains source-built; macOS uses Metal by default.
