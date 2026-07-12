# Intel macOS Whisper Safety Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent Whisper model-load crashes on Intel macOS while preserving Apple Silicon Metal acceleration and accurate hardware classification.

**Architecture:** Make compile-target architecture part of the backend selection boundary: macOS `aarch64` selects Metal, while macOS `x86_64` selects CPU before a Whisper context is created. Reuse the existing `sysinfo` dependency to replace the hard-coded 8 GB memory estimate with installed physical memory.

**Tech Stack:** Rust 2021, whisper-rs 0.13.2, whisper.cpp Metal/CPU backends, sysinfo 0.32, Cargo tests.

---

## File map

- `frontend/src-tauri/src/whisper_engine/acceleration.rs`: select the safe compiled backend and hold its regression tests.
- `frontend/src-tauri/src/audio/hardware_detector.rs`: detect physical RAM and hold conversion tests.
- `frontend/src-tauri/examples/intel_whisper_smoke.rs`: temporary, uncommitted model-backed verification harness; delete after the smoke test.
- `docs/superpowers/specs/2026-07-13-intel-macos-whisper-safety-design.md`: approved design; no further changes expected.

### Task 1: Select CPU on Intel macOS

**Files:**
- Modify: `frontend/src-tauri/src/whisper_engine/acceleration.rs:12-24`
- Test: `frontend/src-tauri/src/whisper_engine/acceleration.rs:83-145`

- [ ] **Step 1: Write the Intel macOS regression test**

Append this test to the existing `tests` module:

```rust
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
#[test]
fn current_backend_uses_cpu_on_intel_macos() {
    assert_eq!(
        WhisperCompiledBackend::current(),
        WhisperCompiledBackend::Cpu
    );
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  cargo test -p meetily current_backend_uses_cpu_on_intel_macos --lib
```

Expected: FAIL because the current implementation returns `Metal` on this `x86_64-apple-darwin` host.

- [ ] **Step 3: Implement architecture-safe backend selection**

Replace the macOS/Metal tail of `WhisperCompiledBackend::current()` with:

```rust
} else if cfg!(target_os = "macos") {
    if cfg!(target_arch = "aarch64") {
        Self::Metal
    } else {
        Self::Cpu
    }
} else if cfg!(feature = "metal") {
    Self::Metal
} else {
    Self::Cpu
}
```

Do not change CUDA, Vulkan, or HIP BLAS precedence.

- [ ] **Step 4: Run acceleration tests and verify GREEN**

Run:

```bash
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  cargo test -p meetily whisper_engine::acceleration::tests --lib
```

Expected: all acceleration tests PASS, including `current_backend_uses_cpu_on_intel_macos`.

- [ ] **Step 5: Commit the backend fix**

```bash
git add frontend/src-tauri/src/whisper_engine/acceleration.rs
git commit -m "fix(transcription): disable Metal on Intel macOS"
```

### Task 2: Detect installed physical memory

**Files:**
- Modify: `frontend/src-tauri/src/audio/hardware_detector.rs:109-119`
- Test: `frontend/src-tauri/src/audio/hardware_detector.rs:277-325`

- [ ] **Step 1: Write byte-conversion tests**

Append these tests to the existing `tests` module:

```rust
#[test]
fn memory_bytes_convert_to_installed_gib() {
    const GIB: u64 = 1024 * 1024 * 1024;

    assert_eq!(HardwareProfile::memory_bytes_to_gb(16 * GIB), 16);
    assert_eq!(HardwareProfile::memory_bytes_to_gb(15 * GIB + 1), 16);
}

#[test]
fn zero_memory_bytes_use_conservative_fallback() {
    assert_eq!(HardwareProfile::memory_bytes_to_gb(0), 8);
}
```

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```bash
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  cargo test -p meetily memory_bytes_ --lib
```

Expected: compilation FAILS because `HardwareProfile::memory_bytes_to_gb` does not exist.

- [ ] **Step 3: Implement physical-memory detection**

Replace `detect_memory_gb()` and add the helper beside it:

```rust
fn detect_memory_gb() -> u8 {
    if let Ok(mem_str) = std::env::var("MEMORY_GB") {
        return mem_str.parse().unwrap_or(8);
    }

    let system = sysinfo::System::new_all();
    Self::memory_bytes_to_gb(system.total_memory())
}

fn memory_bytes_to_gb(bytes: u64) -> u8 {
    const GIB: u64 = 1024 * 1024 * 1024;

    if bytes == 0 {
        return 8;
    }

    let rounded_up = bytes.saturating_add(GIB - 1) / GIB;
    rounded_up.min(u8::MAX as u64) as u8
}
```

No new dependency is needed because `sysinfo = "0.32"` is already present.

- [ ] **Step 4: Run hardware tests and verify GREEN**

Run:

```bash
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  cargo test -p meetily audio::hardware_detector::tests --lib
```

Expected: all hardware detector tests PASS, and the local detection log reports `memory_gb: 16`.

- [ ] **Step 5: Commit the memory fix**

```bash
git add frontend/src-tauri/src/audio/hardware_detector.rs
git commit -m "fix(audio): detect installed system memory"
```

### Task 3: Verify repository behavior and real model loading

**Files:**
- Create temporarily: `frontend/src-tauri/examples/intel_whisper_smoke.rs`
- Delete before commit: `frontend/src-tauri/examples/intel_whisper_smoke.rs`

- [ ] **Step 1: Run the full Rust test suite**

Run:

```bash
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer cargo test -p meetily
```

Expected: PASS with no test failures. Existing unrelated compiler warnings may remain.

- [ ] **Step 2: Create a temporary model-backed smoke harness**

Create `frontend/src-tauri/examples/intel_whisper_smoke.rs` with:

```rust
use anyhow::{Context, Result};
use app_lib::audio::decoder::decode_audio_file;
use app_lib::whisper_engine::WhisperEngine;
use std::path::{Path, PathBuf};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
    .format_timestamp_millis()
    .init();

    let mut args = std::env::args().skip(1);
    let models_dir = PathBuf::from(args.next().context("missing models directory")?);
    let model_name = args.next().context("missing model name")?;
    let audio_path = PathBuf::from(args.next().context("missing audio path")?);

    let engine = WhisperEngine::new_with_models_dir(Some(models_dir))?;
    engine.discover_models().await?;
    engine.load_model(&model_name).await?;

    let samples = decode_audio_file(Path::new(&audio_path))?.to_whisper_format();
    let (text, confidence, _) = engine
        .transcribe_audio_with_confidence(samples, Some("en".to_string()))
        .await?;

    println!("model={model_name} confidence={confidence:.3} text={text}");
    engine.unload_model().await;
    Ok(())
}
```

- [ ] **Step 3: Run `large-v3-turbo` on the Intel host**

Run:

```bash
RUST_LOG=info DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  cargo run -p meetily --example intel_whisper_smoke -- \
  frontend/models large-v3-turbo /tmp/meetily-whisper-benchmark/jfk.wav
```

Expected:

- log contains `memory_gb: 16`;
- log contains `compiled_backend=Cpu` and `use_gpu=false`;
- process exits 0 rather than 139;
- output contains the JFK transcription beginning `And so my fellow Americans`.

- [ ] **Step 4: Optionally confirm `large-v3` with the same safe path**

Run when the machine is not under thermal pressure:

```bash
RUST_LOG=info DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  cargo run -p meetily --example intel_whisper_smoke -- \
  frontend/models large-v3 /tmp/meetily-whisper-benchmark/jfk.wav
```

Expected: the same CPU decision, exit 0, and correct JFK text. This verifies safety, not realtime performance.

- [ ] **Step 5: Delete the temporary harness and verify the diff**

Delete `frontend/src-tauri/examples/intel_whisper_smoke.rs` with `apply_patch`, then run:

```bash
git status --short
git diff --check
git diff upstream/devtest...HEAD --stat
```

Expected: no temporary example remains and no whitespace errors are reported.

### Task 4: Publish the verified fix to PR #605

**Files:**
- No production files beyond Tasks 1-2.

- [ ] **Step 1: Confirm branch and commit scope**

Run:

```bash
git status --short --branch
git log --oneline upstream/devtest..HEAD
```

Expected: clean `feature/whisper-streaming` branch containing the approved design, implementation plan, and two focused fix commits on top of the existing PR commits.

- [ ] **Step 2: Push the existing PR branch**

Run:

```bash
git push origin feature/whisper-streaming
```

Expected: `origin/feature/whisper-streaming` advances to the verified local HEAD and PR #605 updates.

- [ ] **Step 3: Add the verification follow-up comment**

Post this body to PR #605:

```markdown
Implemented the Intel macOS safety follow-up from the model-backed test results.

- macOS `x86_64` now selects the CPU Whisper context before model initialization, avoiding the reproducible Intel Metal `SIGSEGV`.
- macOS `aarch64` continues to select Metal; CUDA, Vulkan, HIP BLAS, and CPU precedence is unchanged.
- hardware detection now reads installed physical memory through the existing `sysinfo` dependency instead of defaulting to 8 GB.
- focused acceleration and hardware tests pass, followed by the full `cargo test -p meetily` suite.
- on the same Intel Mac mini, the official `large-v3-turbo` model now loads through `compiled_backend=Cpu`, transcribes the 11-second JFK sample correctly, and exits normally instead of code 139.

This fixes runtime safety on Intel. It does not claim full unquantized `large-v3` is realtime on Intel CPU; the earlier latency measurements still apply.
```

- [ ] **Step 4: Verify remote state**

Run:

```bash
gh pr view 605 --repo Zackriya-Solutions/meetily \
  --json url,headRefOid,comments,statusCheckRollup
```

Expected: `headRefOid` equals local `HEAD`, the new comment is present, and the PR URL is `https://github.com/Zackriya-Solutions/meetily/pull/605`.
