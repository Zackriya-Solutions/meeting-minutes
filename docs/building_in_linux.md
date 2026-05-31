## 🐧 Building on Linux

This guide helps you build Meetily on Linux with **automatic GPU acceleration**. The build system detects your hardware and configures the best performance automatically.

---

## 🚀 Quick Start (Recommended for Beginners)

If you're new to building on Linux, start here. These simple commands work for most users:

### 1. Install Basic Dependencies

```bash
# Ubuntu/Debian
sudo apt update
sudo apt install build-essential cmake git clang

# Fedora/RHEL
sudo dnf install gcc-c++ cmake git clang

# Arch Linux
sudo pacman -S base-devel cmake git clang
```

> **Note:** `clang` is required on all distros — it provides `libclang.so` which Rust's `bindgen` uses to generate FFI bindings during the build.

### 2. Build and Run

```bash
# Development mode (with hot reload)
./dev-gpu.sh

# Production build
./build-gpu.sh
```

**That's it!** The scripts automatically detect your GPU and configure acceleration.

### What Happens Automatically?

- ✅ **NVIDIA GPU** → CUDA acceleration (if toolkit installed)
- ✅ **AMD GPU** → ROCm acceleration (if ROCm installed)
- ✅ **No GPU** → Optimized CPU mode (still works great!)

> 💡 **Tip:** If you have an NVIDIA or AMD GPU but want better performance, jump to the [GPU Setup](#-gpu-setup-guides-intermediate) section below.

---

## 🧠 Understanding Auto-Detection

The build scripts (`dev-gpu.sh` and `build-gpu.sh`) orchestrate the entire build process. They first call `scripts/auto-detect-gpu.js` to identify your hardware, then build the `llama-helper` sidecar with the appropriate features, and finally launch the Tauri application.

### Detection Priority

| Priority | Hardware        | What It Checks                                               | Result                  |
| -------- | --------------- | ------------------------------------------------------------ | ----------------------- |
| 1️⃣       | **NVIDIA CUDA** | `nvidia-smi` exists + (`CUDA_PATH` or `nvcc` found)          | `--features cuda`       |
| 2️⃣       | **AMD ROCm**    | `rocm-smi` exists + (`ROCM_PATH` or `hipcc` found)           | `--features hipblas`    |
| 3️⃣       | **Vulkan**      | `vulkaninfo` exists + `VULKAN_SDK` + `BLAS_INCLUDE_DIRS` set | `--features vulkan`     |
| 4️⃣       | **OpenBLAS**    | `BLAS_INCLUDE_DIRS` set                                      | `--features openblas`   |
| 5️⃣       | **CPU-only**    | None of the above                                            | (no features, pure CPU) |

### Common Scenarios

| Your System               | Auto-Detection Result       | Why                          |
| ------------------------- | --------------------------- | ---------------------------- |
| Clean Linux install       | CPU-only                    | No GPU SDK detected          |
| NVIDIA GPU + drivers only | CPU-only                    | CUDA toolkit not installed   |
| NVIDIA GPU + CUDA toolkit | **CUDA acceleration** ✅    | Full detection successful    |
| AMD GPU + ROCm            | **HIPBlas acceleration** ✅ | Full detection successful    |
| Vulkan drivers only       | CPU-only                    | Vulkan SDK + env vars needed |
| Vulkan SDK configured     | **Vulkan acceleration** ✅  | All requirements met         |

> 💡 **Key Insight:** Having GPU drivers alone isn't enough. You need the **development SDK** (CUDA toolkit, ROCm, or Vulkan SDK) for acceleration.

---

## 🔧 GPU Setup Guides (Intermediate)

Want better performance? Follow these guides to enable GPU acceleration.

### 🟢 NVIDIA CUDA Setup

**Prerequisites:** NVIDIA GPU with compute capability 5.0+ (check: `nvidia-smi --query-gpu=compute_cap --format=csv`)

#### Step 1: Install CUDA Toolkit

```bash
# Ubuntu/Debian (CUDA 12.x)
sudo apt install nvidia-driver-550 nvidia-cuda-toolkit

# Verify installation
nvidia-smi          # Shows GPU info
nvcc --version      # Shows CUDA version
```

#### Step 2: Build with CUDA

```bash
# Set your GPU's compute capability
# Example: RTX 3080 = 8.6 → use "86"
# Example: GTX 1080 = 6.1 → use "61"

CMAKE_CUDA_ARCHITECTURES=75 \
CMAKE_CUDA_STANDARD=17 \
CMAKE_POSITION_INDEPENDENT_CODE=ON \
./build-gpu.sh
```

> 💡 **Finding Your Compute Capability:**
>
> ```bash
> nvidia-smi --query-gpu=compute_cap --format=csv
> ```
>
> Convert `7.5` → `75`, `8.6` → `86`, etc.

**Why these flags?**

- `CMAKE_CUDA_ARCHITECTURES`: Optimizes for your specific GPU
- `CMAKE_CUDA_STANDARD=17`: Ensures C++17 compatibility
- `CMAKE_POSITION_INDEPENDENT_CODE=ON`: Fixes linking issues on modern systems

---

### 🔵 Vulkan Setup (Cross-Platform Fallback)

Vulkan works on NVIDIA, AMD, and Intel GPUs. Good choice if CUDA/ROCm don't work.

#### Step 1: Install Vulkan SDK and BLAS

```bash
# Ubuntu/Debian
sudo apt install vulkan-sdk libopenblas-dev vulkan-tools

# Fedora
sudo dnf install vulkan-devel openblas-devel vulkan-tools

# Arch Linux — install specific packages from the vulkan-devel group
sudo pacman -S spirv-headers spirv-tools vulkan-headers vulkan-icd-loader vulkan-tools openblas
```

> **Arch Linux note:** `sudo pacman -S vulkan-devel` prompts you to choose from 12 packages.
> Install only: `spirv-headers` (1), `spirv-tools` (2), `vulkan-headers` (6), `vulkan-icd-loader` (8), and `vulkan-tools` (10).
> `vulkan-tools` provides `vulkaninfo`, which the GPU auto-detection script requires to detect Vulkan support.

#### Step 2: Configure Environment

```bash
# bash/zsh — add to ~/.bashrc or ~/.zshrc
export VULKAN_SDK=/usr
export BLAS_INCLUDE_DIRS=/usr/include/openblas   # Arch Linux
# export BLAS_INCLUDE_DIRS=/usr/include/x86_64-linux-gnu  # Ubuntu/Debian

# Apply changes
source ~/.bashrc
```

```fish
# fish shell — sets variables permanently across sessions
set -Ux VULKAN_SDK /usr
set -Ux BLAS_INCLUDE_DIRS /usr/include/openblas   # Arch Linux
# set -Ux BLAS_INCLUDE_DIRS /usr/include/x86_64-linux-gnu  # Ubuntu/Debian
```

> **Arch Linux note:** OpenBLAS headers are at `/usr/include/openblas`, not
> `/usr/include/x86_64-linux-gnu` (that path is Debian/Ubuntu only).

#### Step 3: Build

```bash
./build-gpu.sh
```

The script will automatically detect Vulkan and build with `--features vulkan`.

---

### 🔴 AMD ROCm Setup (AMD GPUs Only)

**Prerequisites:** AMD GPU with ROCm support (RX 5000+, Radeon VII, etc.)

```bash
# Ubuntu/Debian
# Add ROCm repository (see https://rocm.docs.amd.com for latest)
sudo apt install rocm-smi hipcc

# Set environment
export ROCM_PATH=/opt/rocm

# Verify
rocm-smi            # Shows GPU info
hipcc --version     # Shows ROCm version

# Build
./build-gpu.sh
```

---

## 🎯 Advanced Usage

### Manual Feature Override

Want to force a specific acceleration method? Use the `TAURI_GPU_FEATURE` environment variable with the shell scripts:

```bash
# Force CUDA (ignore auto-detection)
TAURI_GPU_FEATURE=cuda ./dev-gpu.sh
TAURI_GPU_FEATURE=cuda ./build-gpu.sh

# Force Vulkan
TAURI_GPU_FEATURE=vulkan ./dev-gpu.sh
TAURI_GPU_FEATURE=vulkan ./build-gpu.sh

# Force ROCm (HIPBlas)
TAURI_GPU_FEATURE=hipblas ./dev-gpu.sh
TAURI_GPU_FEATURE=hipblas ./build-gpu.sh

# Force CPU-only (for testing)
TAURI_GPU_FEATURE="" ./dev-gpu.sh
TAURI_GPU_FEATURE="" ./build-gpu.sh

# Force OpenBLAS (CPU-optimized)
TAURI_GPU_FEATURE=openblas ./dev-gpu.sh
TAURI_GPU_FEATURE=openblas ./build-gpu.sh
```

### Build Output Location

After successful build:

```
src-tauri/target/release/bundle/appimage/Meetily_<version>_amd64.AppImage
```

---

## 🧭 Troubleshooting

### "CUDA toolkit not found"

- **Fix:** Install `nvidia-cuda-toolkit` or set `CUDA_PATH` environment variable
- **Check:** `nvcc --version` should work

### "Vulkan detected but missing dependencies"

- **Fix:** Set both `VULKAN_SDK` and `BLAS_INCLUDE_DIRS` environment variables
- **Also required:** `vulkaninfo` must be in PATH — install `vulkan-tools` (Arch) or `vulkan-utils` (Debian/Ubuntu)
- **Example (bash/zsh):**
  ```bash
  export VULKAN_SDK=/usr
  export BLAS_INCLUDE_DIRS=/usr/include/openblas          # Arch Linux
  # export BLAS_INCLUDE_DIRS=/usr/include/x86_64-linux-gnu  # Ubuntu/Debian
  ```
- **Example (fish):**
  ```fish
  set -Ux VULKAN_SDK /usr
  set -Ux BLAS_INCLUDE_DIRS /usr/include/openblas
  ```

### Rust compile errors: `no field 'greedy' on type whisper_full_params`

This happens on distros that have a system-wide `whisper.cpp` installed (e.g. Arch Linux's
`whisper.cpp` package). The `whisper-rs-sys` build script runs `bindgen` against the system
`/usr/include/whisper.h` (version 1.8.5+) instead of the bundled headers, generating
bindings that are incompatible with `whisper-rs 0.13.2`.

**Fix:** Set `WHISPER_DONT_GENERATE_BINDINGS` to make the build script use its pre-packaged
bindings (which match the bundled whisper.cpp source) instead of regenerating them.

This is already set in `src-tauri/.cargo/config.toml` for this repo. If you're still seeing
the error, the build cache from a previous failed attempt may be stale. Clear it and rebuild:

```bash
# From the project root
find target -type d -name "whisper-rs-sys-*" -exec rm -rf {} + 2>/dev/null
./build-gpu.sh
```

### "AppImage build stripping symbols"

- **Fix:** Already handled! `build-gpu.sh` sets `NO_STRIP=true` automatically
- **Why:** Prevents runtime errors from missing symbols

### Build works but no GPU acceleration

- **Check detection:** Look at the build output for GPU detection messages
- **Verify:** `nvidia-smi` (NVIDIA) or `rocm-smi` (AMD) should work
- **Missing SDK:** Install the development toolkit, not just drivers

---

## 📊 Technical Reference

### Complete Feature Matrix

| Mode     | Feature Flag          | Requirements                                      | Acceleration  | Speed Boost   |
| -------- | --------------------- | ------------------------------------------------- | ------------- | ------------- |
| CUDA     | `--features cuda`     | `nvidia-smi` + (`CUDA_PATH` or `nvcc`)            | GPU           | 5-10x         |
| ROCm     | `--features hipblas`  | `rocm-smi` + (`ROCM_PATH` or `hipcc`)             | GPU           | 4-8x          |
| Vulkan   | `--features vulkan`   | `vulkaninfo` + `VULKAN_SDK` + `BLAS_INCLUDE_DIRS` | GPU           | 3-6x          |
| OpenBLAS | `--features openblas` | `BLAS_INCLUDE_DIRS`                               | CPU-optimized | 1.5-2x        |
| CPU      | (none)                | (none)                                            | CPU-only      | 1x (baseline) |

### Build Scripts Internals

Both `dev-gpu.sh` and `build-gpu.sh` work the same way:

1. **Detect location:** Find `package.json` (works from project root or `frontend/`)
2. **Choose package manager:** Prefer `pnpm`, fallback to `npm`
3. **Call npm script:** Run `tauri:dev` or `tauri:build`
4. **Auto-detect GPU:** The npm script calls `scripts/tauri-auto.js`
5. **Feature selection:** `scripts/auto-detect-gpu.js` checks hardware
6. **Build with features:** Tauri builds with detected `--features` flag

### Environment Variables Reference

| Variable                          | Purpose                             | Example                         |
| --------------------------------- | ----------------------------------- | ------------------------------- |
| `CUDA_PATH`                       | CUDA installation directory         | `/usr/local/cuda`               |
| `ROCM_PATH`                       | ROCm installation directory         | `/opt/rocm`                     |
| `VULKAN_SDK`                      | Vulkan SDK directory                | `/usr`                          |
| `BLAS_INCLUDE_DIRS`               | BLAS headers location               | `/usr/include/openblas` (Arch), `/usr/include/x86_64-linux-gnu` (Debian) |
| `CMAKE_CUDA_ARCHITECTURES`        | GPU compute capability              | `75` (for compute 7.5)          |
| `CMAKE_CUDA_STANDARD`             | C++ standard for CUDA               | `17`                            |
| `CMAKE_POSITION_INDEPENDENT_CODE` | Enable PIC for linking              | `ON`                            |
| `NO_STRIP`                        | Prevent symbol stripping (AppImage) | `true`                          |

---

## ✅ Complete Example Builds

### NVIDIA GPU (CUDA)

```bash
# Install
sudo apt install nvidia-driver-550 nvidia-cuda-toolkit

# Verify
nvidia-smi --query-gpu=compute_cap --format=csv

# Build (adjust architecture for your GPU)
CMAKE_CUDA_ARCHITECTURES=86 \ # (86 may change in your case)
CMAKE_CUDA_STANDARD=17 \
CMAKE_POSITION_INDEPENDENT_CODE=ON \
./build-gpu.sh
```

### AMD GPU (ROCm)

```bash
# Install ROCm (see AMD docs for your distro)
sudo apt install rocm-smi hipcc
export ROCM_PATH=/opt/rocm

# Build
./build-gpu.sh
```

### Any GPU (Vulkan)

```bash
# Install (Ubuntu/Debian)
sudo apt install vulkan-sdk libopenblas-dev vulkan-tools

# Install (Arch Linux)
sudo pacman -S spirv-headers spirv-tools vulkan-headers vulkan-icd-loader vulkan-tools openblas

# Configure (bash/zsh)
export VULKAN_SDK=/usr
export BLAS_INCLUDE_DIRS=/usr/include/openblas          # Arch Linux
# export BLAS_INCLUDE_DIRS=/usr/include/x86_64-linux-gnu  # Ubuntu/Debian

# Configure (fish)
# set -Ux VULKAN_SDK /usr
# set -Ux BLAS_INCLUDE_DIRS /usr/include/openblas

# Build
./build-gpu.sh
```

### No GPU (CPU-only)

```bash
# Just build - works out of the box
./build-gpu.sh
```

---

**Need help?** Open an issue on GitHub with your GPU type, distro, and the output from `./build-gpu.sh`.
