# 🐧 Linux Troubleshooting Guide

This guide covers common issues when building or running Meetily on Linux.

---

## 📦 Build Issues

### "cargo: command not found"

**Cause:** Rust is not installed.

**Fix:**
```bash
# Ubuntu/Debian
sudo apt install rustc cargo

# Arch Linux
sudo pacman -S rust

# Fedora
sudo dnf install rust cargo

# Or via rustup (recommended for all distros)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

---

### "Unable to find libclang" / "couldn't find any valid shared libraries matching libclang.so"

**Cause:** The `clang` development libraries are missing. The Rust `bindgen` crate requires libclang to generate FFI bindings.

**Fix:**
```bash
# Ubuntu/Debian
sudo apt install clang libclang-dev

# Arch Linux
sudo pacman -S clang

# Fedora
sudo dnf install clang clang-devel
```

If the error persists, set the path manually:
```bash
# Find libclang
find /usr -name "libclang.so*" 2>/dev/null

# Set the path (adjust based on find result)
export LIBCLANG_PATH=/usr/lib/llvm-14/lib  # Ubuntu example
export LIBCLANG_PATH=/usr/lib              # Arch example
```

---

### "pnpm: command not found"

**Cause:** pnpm package manager is not installed.

**Fix:**
```bash
# Via npm (all distros)
sudo npm install -g pnpm

# Arch Linux (native package)
sudo pacman -S pnpm

# Via corepack (Node.js 16.13+)
corepack enable
corepack prepare pnpm@latest --activate
```

---

### "tauri: command not found" / "beforeBuildCommand failed"

**Cause:** Tauri CLI is not installed or npm dependencies are missing.

**Fix:**
```bash
cd frontend
pnpm install  # Installs @tauri-apps/cli locally

# Or install globally
pnpm install -g @tauri-apps/cli
```

---

### "CUDA toolkit not found" (but NVIDIA GPU detected)

**Cause:** The CUDA development toolkit is not installed. Having only the NVIDIA driver is not enough.

**Fix:**
```bash
# Ubuntu/Debian
sudo apt install nvidia-cuda-toolkit

# Arch Linux
sudo pacman -S cuda

# Fedora (requires RPM Fusion)
sudo dnf install cuda

# Verify installation
nvcc --version
```

If `nvcc` works but the build still fails, set the CUDA path:
```bash
export CUDA_PATH=/usr/local/cuda  # Ubuntu/Debian
export CUDA_PATH=/opt/cuda        # Arch Linux
```

---

### "Failed to set DNS configuration" (during VPN setup, not Meetily)

**Cause:** If you're using WireGuard VPN, `systemd-resolved` may not be active.

**Fix:**
```bash
# Option 1: Enable systemd-resolved
sudo systemctl enable --now systemd-resolved

# Option 2: Install openresolv (if not conflicting)
sudo pacman -S openresolv  # Arch
sudo apt install openresolv  # Debian/Ubuntu
```

---

## 🖥️ Runtime Issues

### White/blank screen after launch

**Cause:** WebKit2GTK has issues with GPU compositing on certain hardware configurations, particularly with NVIDIA GPUs.

**Symptoms:**
- App window opens but shows only white/blank content
- Error in terminal: `Failed to create GBM buffer of size XXXxYYY: Invalid argument`

**Quick Fix (temporary):**
```bash
WEBKIT_DISABLE_COMPOSITING_MODE=1 ./meetily.AppImage
```

**Permanent Fix (desktop entry):**

Edit your `.desktop` file to include the environment variable:
```ini
[Desktop Entry]
Name=Meetily
Exec=env WEBKIT_DISABLE_COMPOSITING_MODE=1 /path/to/meetily.AppImage
...
```

**Alternative fixes to try:**
```bash
# Disable DMA-BUF renderer
WEBKIT_DISABLE_DMABUF_RENDERER=1 ./meetily.AppImage

# Force software rendering (slower but most compatible)
LIBGL_ALWAYS_SOFTWARE=1 ./meetily.AppImage
```

---

### App crashes immediately after launch

**Cause:** Missing runtime dependencies.

**Fix:** Install WebKit and GTK runtime libraries:
```bash
# Ubuntu/Debian
sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0

# Arch Linux
sudo pacman -S webkit2gtk-4.1 gtk3

# Fedora
sudo dnf install webkit2gtk4.1 gtk3
```

---

### No audio devices detected

**Cause:** PulseAudio/PipeWire permissions or missing ALSA libraries.

**Fix:**
```bash
# Check audio server is running
pactl info  # PulseAudio
pw-cli info  # PipeWire

# Install ALSA development libraries
sudo apt install libasound2-dev  # Debian/Ubuntu
sudo pacman -S alsa-lib          # Arch
sudo dnf install alsa-lib-devel  # Fedora
```

---

### "Permission denied" when accessing microphone

**Cause:** AppImage sandbox or Flatpak permissions.

**Fix:**
```bash
# For AppImage, extract and run directly
./meetily.AppImage --appimage-extract
./squashfs-root/AppRun

# Or run with --no-sandbox (less secure)
./meetily.AppImage --no-sandbox
```

---

## 🎮 GPU-Specific Issues

### NVIDIA: "CUDA not detected" despite having GPU

**Checklist:**
1. NVIDIA driver installed: `nvidia-smi` should work
2. CUDA toolkit installed: `nvcc --version` should work
3. Environment variables set:
   ```bash
   export CUDA_PATH=/usr/local/cuda
   export PATH=$CUDA_PATH/bin:$PATH
   export LD_LIBRARY_PATH=$CUDA_PATH/lib64:$LD_LIBRARY_PATH
   ```

---

### AMD: "ROCm not detected"

**Checklist:**
1. ROCm installed: `rocm-smi` should work
2. HIP compiler available: `hipcc --version` should work
3. Environment set:
   ```bash
   export ROCM_PATH=/opt/rocm
   export PATH=$ROCM_PATH/bin:$PATH
   ```

---

### Vulkan: "Vulkan detected but missing dependencies"

**Cause:** Environment variables not set.

**Fix:**
```bash
# Add to ~/.bashrc or ~/.zshrc
export VULKAN_SDK=/usr
export BLAS_INCLUDE_DIRS=/usr/include/x86_64-linux-gnu  # Debian/Ubuntu
export BLAS_INCLUDE_DIRS=/usr/include                    # Arch

source ~/.bashrc
```

---

## 🔧 Distribution-Specific Issues

### Arch Linux

**Issue:** Package names differ from Ubuntu/Debian documentation.

**Package mapping:**
| Ubuntu/Debian | Arch Linux |
|---------------|------------|
| `nvidia-cuda-toolkit` | `cuda` |
| `libwebkit2gtk-4.1-dev` | `webkit2gtk-4.1` |
| `libgtk-3-dev` | `gtk3` |
| `build-essential` | `base-devel` |
| `libclang-dev` | `clang` |

---

### Fedora/RHEL

**Issue:** Some packages require RPM Fusion repository.

**Fix:**
```bash
# Enable RPM Fusion
sudo dnf install https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-$(rpm -E %fedora).noarch.rpm
sudo dnf install https://mirrors.rpmfusion.org/nonfree/fedora/rpmfusion-nonfree-release-$(rpm -E %fedora).noarch.rpm

# Then install NVIDIA drivers
sudo dnf install nvidia-driver cuda
```

---

### NixOS

**Issue:** Standard installation doesn't work due to NixOS's unique package management.

**Fix:** Use a Nix flake or shell:
```nix
{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
  buildInputs = with pkgs; [
    rustc cargo cmake clang
    webkitgtk_4_1 gtk3
    pkg-config openssl
  ];
}
```

---

## 📋 Diagnostic Commands

Run these to gather information for bug reports:

```bash
# System info
uname -a
cat /etc/os-release

# GPU info
lspci | grep -i vga
nvidia-smi 2>/dev/null || echo "No NVIDIA driver"
rocm-smi 2>/dev/null || echo "No ROCm"
vulkaninfo --summary 2>/dev/null || echo "No Vulkan"

# Rust/Node versions
rustc --version
cargo --version
node --version
pnpm --version

# WebKit version
pkg-config --modversion webkit2gtk-4.1

# Check for libclang
find /usr -name "libclang.so*" 2>/dev/null
```

---

## 🆘 Getting Help

If none of these solutions work:

1. **Gather diagnostics:** Run the commands above
2. **Check build output:** Save the full output of `./build-gpu.sh`
3. **Open an issue:** https://github.com/Zackriya-Solutions/meeting-minutes/issues

Include:
- Your distribution and version
- GPU type and driver version
- Full error message
- Output of diagnostic commands
