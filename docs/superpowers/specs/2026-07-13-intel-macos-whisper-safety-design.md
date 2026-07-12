# Intel macOS Whisper Acceleration Safety

## Context

On an Intel Mac mini, loading both `large-v3-turbo` and `large-v3` through the
Meetily macOS Metal build reproducibly exits with `SIGSEGV` before inference.
The hardware detector classifies the machine as having no supported Metal
acceleration, but `WhisperCompiledBackend::current()` still selects Metal for
every macOS target. The context loader then passes `use_gpu = true` to
`whisper-rs`.

The same models load and transcribe correctly when `use_gpu = false`. This
shows that the model files and streaming reconciliation are sound and isolates
the crash to the Intel Metal initialization path.

## Goals

- Never attempt the known-crashing Metal context on Intel macOS.
- Preserve Metal acceleration on Apple Silicon.
- Preserve the existing CUDA, Vulkan, HIP BLAS, and CPU behavior.
- Report installed physical memory instead of defaulting to 8 GB.
- Cover the architecture choice and memory conversion with focused tests.

## Non-goals

- Making full-size `large-v3` real-time on Intel CPUs.
- Adding OpenVINO, SYCL, or a helper subprocess for GPU probing.
- Changing the model catalogue or automatic model selection.
- Upgrading `whisper-rs` or its bundled `whisper.cpp` in this patch.

## Considered approaches

### 1. Architecture-safe backend selection (selected)

Treat the compiled macOS backend as Metal only on `aarch64`; select CPU on
`x86_64`. This is the smallest change, matches the existing hardware detector,
and prevents the fatal path before model initialization.

### 2. Try Metal and retry on CPU

This would preserve possible Intel Metal acceleration when initialization
returns an ordinary error. It cannot protect Meetily from the observed
`SIGSEGV`, because the process terminates before Rust can retry. It is therefore
not safe as the primary fallback.

### 3. Probe Metal out of process or add an Intel-specific backend

A helper process could contain a Metal crash, while OpenVINO or a newer engine
could potentially improve Intel performance. Both approaches add packaging,
state, and cross-platform maintenance work and should be evaluated separately.

## Design

`WhisperCompiledBackend::current()` will select:

- Metal for macOS on `aarch64`;
- CPU for macOS on `x86_64`;
- the existing feature-selected backend on other platforms.

No in-process Metal attempt will be made on Intel macOS. The existing
`WhisperContextParameters` construction will therefore receive `use_gpu =
false` through the normal acceleration decision, without a special case in the
model loader.

`HardwareProfile::detect_memory_gb()` will retain `MEMORY_GB` as an explicit
development/test override. Without the override it will read physical memory
through the already-installed `sysinfo` dependency, convert bytes to GiB, round
up partial GiB, and clamp the result to the `u8` range used by the profile.

## Error handling and observability

The existing acceleration decision log remains the source of truth. On Intel
macOS it must report `compiled_backend=Cpu` and `use_gpu=false`. Model load
errors continue through the existing `Result` path; this patch only prevents
the fatal backend choice that bypasses error handling.

## Verification

The change will follow test-driven development:

1. Add an Intel-macOS-only regression test that expects the current backend to
   be CPU and observe it fail before changing production code.
2. Add unit tests for byte-to-GiB conversion, including a 16 GB machine and a
   partial-GiB value, and observe the missing helper failure.
3. Implement the minimal backend and memory-detection changes.
4. Run the focused tests, all `whisper_engine`/hardware tests, and
   `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer cargo test -p
   meetily`.
5. Run the existing model-backed Intel smoke test with `large-v3-turbo` to
   prove the branch no longer crashes and uses CPU. Re-test `large-v3` if time
   and thermal conditions permit; it is already known to work in CPU mode.

## Acceptance criteria

- Intel macOS does not enter Whisper Metal initialization.
- `large-v3-turbo` loads and transcribes without process termination.
- Apple Silicon still selects Metal at compile time.
- A 16 GB machine is classified using 16 GB rather than the 8 GB fallback.
- All repository tests pass and the working tree contains only intentional
  changes.
