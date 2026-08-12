# GPU Acceleration Guide

Meetily supports GPU acceleration for transcription and built-in local summarization. GPU acceleration backends are selected at build time, and CUDA and Vulkan are independent Cargo features.

## Supported Backends

Meetily has more than one local inference engine:

* **Whisper transcription** uses `whisper-rs`, a Rust wrapper around `whisper.cpp`.
* **Parakeet transcription** uses ONNX Runtime (`ort` is its Rust interface).
* **Built-in Qwen/GGUF summarization** runs in the `llama-helper` sidecar through `llama.cpp`.

The supported build backends are:

* **CUDA:** NVIDIA GPU acceleration. It enables CUDA for Whisper and ONNX Runtime in the Tauri application; `llama-helper` must also be built with its CUDA feature to accelerate built-in summarization.
* **Metal/Core ML:** Apple acceleration.
* **Vulkan:** A separate Whisper backend for modern AMD and Intel GPUs on Windows/Linux.
* **OpenBLAS:** CPU optimization rather than GPU acceleration.

Parakeet currently supports CPU or CUDA in Meetily. Selecting Vulkan does not enable Vulkan for Parakeet.

### Acceleration coverage

| Build | Whisper transcription | Parakeet transcription | Built-in Qwen/GGUF summarization |
| --- | --- | --- | --- |
| CPU | CPU | CPU | CPU |
| CUDA | CUDA through `whisper.cpp` | ONNX Runtime CUDA, with CPU fallback | CUDA through `llama.cpp`, with CPU fallback |
| Vulkan | Vulkan through `whisper.cpp` | CPU | Requires a separately Vulkan-enabled `llama-helper` build |
| Metal | Metal through `whisper.cpp` | CPU | Metal through `llama.cpp` |

Installing or detecting the CUDA Toolkit does not change an application that was already built. Parakeet uses the NVIDIA GPU only when Meetily is compiled with the `cuda` Cargo feature—which enables `ort/cuda`—and ONNX Runtime successfully initializes its CUDA execution provider. Otherwise, Parakeet uses its CPU provider.

## Automatic GPU Detection

The build scripts (`dev-gpu.sh`, `build-gpu.sh`) are designed to automatically detect your GPU and enable the appropriate feature flag during the build process. The detection is handled by the `scripts/auto-detect-gpu.js` script.

Here's the detection priority:

1.  **CUDA (NVIDIA)**
2.  **Metal (Apple)**
3.  **Vulkan (AMD/Intel)**
4.  **OpenBLAS (CPU)**

If no supported GPU backend is available, the application falls back to CPU processing. CUDA Parakeet sessions prefer the CUDA execution provider and keep the CPU provider as fallback. Built-in summarization retries model loading once on CPU if CUDA loading fails.

## Manual Configuration

GPU backends are selected with Cargo features. Passing `--features cuda` does not enable unrelated features such as Vulkan.

The existing Cargo feature mapping is:

```toml
[features]
cuda = ["whisper-rs/cuda", "ort/cuda"]
vulkan = ["whisper-rs/vulkan"]
```

Do not edit `default` to select a backend. Pass the desired feature to the build command instead.

## Platform-Specific Instructions

### Linux

For detailed instructions on setting up GPU acceleration on Linux, please refer to the [Linux build instructions](BUILDING.md#--building-on-linux).

### macOS

On macOS, Metal GPU acceleration is enabled by default. No additional configuration is required.

### Windows

See the [Windows build instructions](BUILDING.md#-building-on-windows) for NVIDIA CUDA and AMD/Intel Vulkan prerequisites and commands.
