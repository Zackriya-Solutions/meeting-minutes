## 🐧 Building on Linux

This guide helps you build Meetily on Linux with **automatic GPU acceleration**. The build system detects your hardware and configures the best performance automatically.

---

## 🚀 Quick Start (Recommended for Beginners)

If you're new to building on Linux, start here. These simple commands work for most users:

### 1. Install Basic Dependencies

```bash
# Ubuntu/Debian
sudo apt update
sudo apt install build-essential cmake git

# Fedora/RHEL
sudo dnf install gcc-c++ cmake git

# Arch Linux
sudo pacman -S base-devel cmake git
```

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

The build scripts (`dev-gpu.sh` and `build-gpu.sh`) call `scripts/auto-detect-gpu.js` which automatically detects your hardware and selects the best acceleration method.

### Detection Priority

| Priority | Hardware | What It Checks | Result |
|----------|----------|----------------|--------|
| 1️⃣ | **NVIDIA CUDA** | `nvidia-smi` exists + (`CUDA_PATH` or `nvcc` found) | `--features cuda` |
| 2️⃣ | **AMD ROCm** | `rocm-smi` exists + (`ROCM_PATH` or `hipcc` found) | `--features hipblas` |
| 3️⃣ | **Vulkan** | `vulkaninfo` exists + `VULKAN_SDK` + `BLAS_INCLUDE_DIRS` set | `--features vulkan` |
| 4️⃣ | **OpenBLAS** | `BLAS_INCLUDE_DIRS` set | `--features openblas` |
| 5️⃣ | **CPU-only** | None of the above | (no features, pure CPU) |

### Common Scenarios

| Your System | Auto-Detection Result | Why |
|-------------|----------------------|-----|
| Clean Linux install | CPU-only | No GPU SDK detected |
| NVIDIA GPU + drivers only | CPU-only | CUDA toolkit not installed |
| NVIDIA GPU + CUDA toolkit | **CUDA acceleration** ✅ | Full detection successful |
| AMD GPU + ROCm | **HIPBlas acceleration** ✅ | Full detection successful |
| Vulkan drivers only | CPU-only | Vulkan SDK + env vars needed |
| Vulkan SDK configured | **Vulkan acceleration** ✅ | All requirements met |

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
> ```bash
> nvidia-smi --query-gpu=compute_cap --format=csv
> ```
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
sudo apt install vulkan-sdk libopenblas-dev

# Fedora
sudo dnf install vulkan-devel openblas-devel

# Arch Linux
sudo pacman -S vulkan-devel openblas
```

#### Step 2: Configure Environment

```bash
# Add to ~/.bashrc or ~/.zshrc
export VULKAN_SDK=/usr
export BLAS_INCLUDE_DIRS=/usr/include/x86_64-linux-gnu

# Apply changes
source ~/.bashrc
```

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

Want to force a specific acceleration method? Use these commands:

```bash
# Force CUDA (ignore auto-detection)
pnpm run tauri:dev:cuda
pnpm run tauri:build:cuda

# Force Vulkan
pnpm run tauri:dev:vulkan
pnpm run tauri:build:vulkan

# Force ROCm (HIPBlas)
pnpm run tauri:dev:hipblas
pnpm run tauri:build:hipblas

# Force CPU-only (for testing)
pnpm run tauri:dev:cpu
pnpm run tauri:build:cpu

# Force OpenBLAS (CPU-optimized)
pnpm run tauri:dev:openblas
pnpm run tauri:build:openblas
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
- **Example:**
  ```bash
  export VULKAN_SDK=/usr
  export BLAS_INCLUDE_DIRS=/usr/include/x86_64-linux-gnu
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

| Mode     | Feature Flag          | Requirements                                    | Acceleration  | Speed Boost |
|----------|-----------------------|-------------------------------------------------|---------------|-------------|
| CUDA     | `--features cuda`     | `nvidia-smi` + (`CUDA_PATH` or `nvcc`)          | GPU           | 5-10x       |
| ROCm     | `--features hipblas`  | `rocm-smi` + (`ROCM_PATH` or `hipcc`)           | GPU           | 4-8x        |
| Vulkan   | `--features vulkan`   | `vulkaninfo` + `VULKAN_SDK` + `BLAS_INCLUDE_DIRS` | GPU        | 3-6x        |
| OpenBLAS | `--features openblas` | `BLAS_INCLUDE_DIRS`                             | CPU-optimized | 1.5-2x      |
| CPU      | (none)                | (none)                                          | CPU-only      | 1x (baseline) |

### Build Scripts Internals

Both `dev-gpu.sh` and `build-gpu.sh` work the same way:

1. **Detect location:** Find `package.json` (works from project root or `frontend/`)
2. **Choose package manager:** Prefer `pnpm`, fallback to `npm`
3. **Call npm script:** Run `tauri:dev` or `tauri:build`
4. **Auto-detect GPU:** The npm script calls `scripts/tauri-auto.js`
5. **Feature selection:** `scripts/auto-detect-gpu.js` checks hardware
6. **Build with features:** Tauri builds with detected `--features` flag

### Environment Variables Reference

| Variable | Purpose | Example |
|----------|---------|---------|
| `CUDA_PATH` | CUDA installation directory | `/usr/local/cuda` |
| `ROCM_PATH` | ROCm installation directory | `/opt/rocm` |
| `VULKAN_SDK` | Vulkan SDK directory | `/usr` |
| `BLAS_INCLUDE_DIRS` | BLAS headers location | `/usr/include/x86_64-linux-gnu` |
| `CMAKE_CUDA_ARCHITECTURES` | GPU compute capability | `75` (for compute 7.5) |
| `CMAKE_CUDA_STANDARD` | C++ standard for CUDA | `17` |
| `CMAKE_POSITION_INDEPENDENT_CODE` | Enable PIC for linking | `ON` |
| `NO_STRIP` | Prevent symbol stripping (AppImage) | `true` |

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
# Install
sudo apt install vulkan-sdk libopenblas-dev

# Configure
export VULKAN_SDK=/usr
export BLAS_INCLUDE_DIRS=/usr/include/x86_64-linux-gnu

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
