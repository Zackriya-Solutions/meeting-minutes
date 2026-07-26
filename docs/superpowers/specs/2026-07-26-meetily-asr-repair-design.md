# Meetily ASR: Repair the Local Transcription Path

**Date:** 2026-07-26
**Status:** Approved
**Scope:** Restore a healthy build and a GPU-accelerated local transcription path on Windows. Remove the abandoned llama-server ASR provider. No new ASR model in this pass.

## Context

The working tree carried 14 modified files plus one new file implementing a `custom` ASR
provider backed by a local `llama-server.exe` serving Qwen3-ASR-1.7B or Voxtral-Mini-3B GGUF.
Recording/transcription stopped working and transcript quality degraded.

Target machine: RTX 5000 Ada Laptop (16 GB VRAM), i9-13950HX (24 cores), 128 GB RAM,
CUDA Toolkit 13.0, Visual Studio 18 Insiders. Meetings are predominantly English.

## Diagnosis

1. **Build is broken.** `cargo check` exits 101. `whisper-rs-sys` bindgen fails with
   `fatal error: 'stdbool.h' file not found`, falls back to stale bundled bindings, and
   those produce `error[E0080]` const-eval overflows against the vendored `whisper.cpp`
   struct layouts. Root cause: `LIBCLANG_PATH` points at the Python pip `clang` package
   (`site-packages/clang/native`), which ships only `libclang.dll` with no clang resource
   headers. Last successful build artifact dates from 2026-07-24 21:42.

2. **GPU is unused.** The build script reports `Windows: Using CPU-only mode` and
   `NVIDIA GPU detected! Consider rebuilding with --features cuda`. The default
   `pnpm tauri:dev` target compiles `whisper-rs` without the `cuda` feature.

3. **Beam-size regression (introduced by the diff).** `HardwareProfile::has_cuda_support()`
   returns `true` whenever the `CUDA_PATH` environment variable exists — it never checks
   whether `whisper-rs` was compiled with the `cuda` feature. The diff changed the Windows
   branch of `get_whisper_config()` from a fixed `beam_size: 2` to
   `if has_gpu_acceleration { 5 } else { 2 }`. On this machine that means **beam search 5
   running on CPU**, roughly 2.5x slower than before, so the transcription queue falls
   behind real time.

4. **The `custom` provider is architecturally unsuited to near-real-time.** It sends each
   audio chunk as base64 WAV through `/v1/chat/completions` with a 120 s timeout and relies
   on free-form generation. `clean_transcript()` already exists to strip prompt echo and
   `<asr_text>` artifacts, which means hallucination was observed. It also hardcodes
   `C:\Work\models\...` paths and runs a global `taskkill /IM llama-server.exe /F`.
   For English-heavy use it is less accurate than the Parakeet engine already shipped.

5. **Unbounded `initial_prompt`.** The new custom-vocabulary feature feeds arbitrary-length
   text into Whisper's `initial_prompt`, a known trigger for repetition and hallucination.

6. **The CUDA build does not configure.** `cargo check --features cuda` fails in CMake with
   `No CUDA toolset found`. CUDA 13.0's MSBuild integration files exist under
   `extras/visual_studio_integration/MSBuildExtensions` but were never copied into
   Visual Studio 18's `BuildCustomizations` directory, so the `Visual Studio 18 2026`
   generator cannot find the CUDA toolset. The configure step also reports
   `Using CUDA architectures: 52;61;70;75`, none of which is this GPU's Ada
   Lovelace `sm_89`, and CUDA 13 has dropped several of those older architectures.

7. **Parakeet is hardcoded to CPU.** `parakeet_engine/model.rs:92` builds its ONNX Runtime
   session with `vec![CPUExecutionProvider::default().build()]` and no GPU execution
   provider. So neither transcription engine touches the RTX 5000 Ada.

## Model research (recorded for a later pass)

Evaluated for "open-source, real / near-real-time, English-heavy, RTX 5000 Ada":

| Model | English WER | Latency | Turkish | Integration cost |
|---|---|---|---|---|
| NVIDIA Nemotron 3.5 ASR streaming 0.6B | ~7.9% (FLEURS) | true cache-aware streaming, ~0.5 s | yes | new provider via `parakeet-rs` 0.3 |
| NVIDIA Parakeet TDT 0.6B v3 | 6.32% (Open ASR) | VAD-segment bound (~2-5 s) | no | already integrated |
| Whisper large-v3-turbo | ~7.8% | chunked, slowest | yes | already integrated |
| Canary-Qwen 2.5B | 5.63% (Open ASR) | offline only | no | heavy, English-only |

Preferred runtime if a new model is added later: the `parakeet-rs` 0.3 crate (ONNX Runtime
based, `cuda`/`tensorrt` cargo features, in-process, no sidecar server). It supports the
Nemotron 3.5 multilingual streaming variant. Rejected: `sherpa-rs` (extra C++ FFI layer),
a NeMo Python sidecar (PyTorch dependency in a desktop app), NIM/Riva (requires Docker,
conflicts with Meetily's single-binary privacy-first design).

**Decision: defer.** Fix the existing stack first and measure Parakeet and Whisper with CUDA
actually enabled before adding a model.

## Plan

1. **Done.** Repaired the bindgen toolchain by supplying the MSVC and Windows SDK include
   directories through `BINDGEN_EXTRA_CLANG_ARGS`, pinned in a gitignored
   `.cargo/config.toml` at the workspace root. Installing LLVM turned out to be
   unnecessary. *Verified:* `cargo check` finishes in a clean environment with no
   `Unable to generate bindings` warning.
2. **Blocked.** See diagnosis item 6. Requires either copying CUDA's MSBuild integration
   into Visual Studio 18 (needs administrator rights) or switching whisper.cpp's CMake
   generator to Ninja, plus setting `CMAKE_CUDA_ARCHITECTURES=89`. Deferred to a decision.
3. **Done.** `has_cuda_support()` and `has_vulkan_support()` now return false unless the
   corresponding cargo feature is compiled in. *Verified:* `gpu_backends_require_their_cargo_feature`
   and `windows_beam_size_tracks_gpu_acceleration` tests pass (7/7 in the module).
4. **Done.** Deleted `custom_asr_provider.rs` and unwired it. `api_get_transcript_config`
   migrates a persisted `custom`/`qwenAsr` provider to parakeet, so `engine.rs`, `mod.rs`
   and `setting.rs` returned exactly to their committed state. *Verified:* `cargo check`
   clean, `tsc --noEmit` clean apart from a pre-existing `bun:test` typing error in
   `tests/lib/blocknote-markdown.test.ts`.
5. **Done.** Custom vocabulary is capped at 800 characters (~200 tokens, below whisper's
   224-token `initial_prompt` limit) with a warning when truncation happens.
6. **Done.** Kept the Next.js `ChunkLoadError` mitigation and the Whisper token-probability
   confidence fix — both are correct.

## GPU acceleration: resolved via DirectML

ort's CUDA execution provider is not viable on this machine. It needs a CUDA 12 runtime
plus cuDNN 9; the box has only CUDA 13.0's compiler tools and no cuDNN at all
(`cudart64_12.dll`, `cudart64_13.dll` and every `cudnn64_*.dll` are absent; only the
driver's `nvcuda.dll` is present). Supplying them means multi-gigabyte system installs.

DirectML needs none of that — any Direct3D 12 device works, and `ort` already ships
`DirectML.dll`. `parakeet_engine/model.rs` now tries DirectML first and falls back to the
CPU provider. The two configurations are built separately because DirectML requires
sequential execution with memory-pattern optimisation disabled, and reusing that
configuration for the CPU path would have made CPU-only Windows machines slower than
before.

*Verified:* `directml_runs_the_installed_parakeet_encoder` builds a DirectML-only session
over the real 622 MB int8 encoder with `error_on_failure()`, which turns a silent CPU
fallback into a test failure. It passes.

Whisper on CUDA remains blocked by diagnosis item 6 and is now lower priority, since the
recommended provider (Parakeet) is GPU-accelerated without it.

## Nemotron 3.5: blocked by a dependency pin

`parakeet-rs` 0.3.6 is the Rust crate with Nemotron 3.5 streaming support, and it requires
`ort ^2.0.0-rc.12`. Meetily's VAD crate, `silero_rs`, pins `ort = "=2.0.0-rc.10"` exactly,
and silero-rs upstream still does (they deliberately downgraded ort in January 2026 for
CUDA image compatibility). Cargo cannot satisfy both in one resolution graph, and VAD is
core to the audio pipeline so it cannot be dropped. Adding `parakeet-rs` also pulls
`ndarray` 0.16 -> 0.17, though that blast radius is a single file.

Options considered:

1. **Vendor and patch silero-rs** — change its ort pin, add `[patch]` at the workspace
   root, migrate Meetily to ndarray 0.17. Touches working VAD.
2. **Separate sidecar process** — a standalone crate outside the workspace with its own
   lockfile, shipped as an `externalBin` next to `llama-helper` and `ffmpeg`. No version
   conflict; adds process lifecycle management.
3. **Skip Nemotron** — Parakeet TDT v3 scores 6.32% WER on the Open ASR Leaderboard
   against Nemotron's ~7.9% on FLEURS English, and is now GPU-accelerated.

**Chosen: option 2.**

## Nemotron 3.5 implementation

`nemotron-helper/` is a standalone crate, listed under `exclude` in the workspace root so
it resolves its own dependency graph. It speaks newline-delimited JSON over stdin/stdout —
`load`, `transcribe`, `ping`, `shutdown` — mirroring the llama-helper protocol. Audio
crosses the pipe as base64 over little-endian f32 samples, which is what the audio
pipeline already produces. It requests the DirectML execution provider, matching the
Parakeet path.

`audio/transcription/nemotron_provider.rs` spawns the sidecar on first use and keeps it
alive, since loading the model costs about 4 seconds. A failed exchange drops the child so
the next call respawns it. The binary is located the same way llama-helper is: the
`MEETILY_NEMOTRON_HELPER` environment variable first, then a scan next to the running
executable.

Model files live in `<app data>/models/nemotron/nemotron-3.5-asr-streaming-0.6b/`
(encoder.onnx, encoder.onnx.data, decoder_joint.onnx, tokenizer.model — 2.48 GB total,
from the `altunenes/parakeet-rs` repository on HuggingFace).

*Verified* against `jfk_norm.wav` (11.0 s, 16 kHz mono):

```
load: 3.62s provider=DirectML
run 1: 1.38s   RTFx   8.0x (warm-up)
run 2: 0.76s   RTFx  14.4x
run 3: 0.77s   RTFx  14.3x
run 4: 0.76s   RTFx  14.5x
```

Transcript: "And so my fellow Americans ask not what your country can do for you.  Ask
what you can do for your country" — correct, with punctuation and capitalisation.

Known limitation: each request is treated as one complete utterance, so the provider gets
Nemotron's accuracy and speed but not yet its incremental partial results. True streaming
would need the transcription pipeline to feed continuous audio rather than VAD segments.

### Bug: lazy sidecar start produced zero transcripts

The first recording against Nemotron saved 0 transcript segments. The log traced it end to
end:

```
🔍 Validating Nemotron model...        (checks the files exist, does not load)
🧠 Initializing Nemotron provider      (constructs the object, does not spawn)
⚠️ Worker 0 pre-validation: Nemotron 3.5 ASR model not loaded - chunks may be skipped
👷 Worker 0 processing chunk 0 with 34720 samples
⚠️ Worker 0: Model unloaded, but continuing to preserve chunk 0
```

`audio/transcription/worker.rs` gates every chunk on `is_model_loaded()` before it will
call `transcribe`. The provider spawned its sidecar lazily inside `transcribe`, so
`is_model_loaded()` stayed false forever: the worker skipped every chunk, which meant
`transcribe` never ran, which meant the sidecar never started. Whisper and Parakeet do not
hit this because their validation step loads the model before recording begins.

Fix: `NemotronProvider::ensure_started()`, called from the `nemotron` arm of
`get_or_init_transcription_engine`. Covered by
`reports_loaded_after_start_without_transcribing_first`, which asserts the provider
reports a loaded model *without* a transcribe call having happened — the exact contract
the worker depends on.

A second suspicion was ruled out by reading rather than assuming: worker.rs applies a 0.30
confidence threshold to `Provider` engines, and this provider returns `confidence: None`.
Line 170 is `confidence_opt.map_or(true, |c| c >= confidence_threshold)`, so `None` is
accepted.

## Bug: VAD cut speech mid-sentence

Transcripts came back as fragments — "Mm-hmm. | This one is just | Then | How do you" —
with most words missing. The engine was not at fault: `ParakeetEngine` transcribes an
11 s clip perfectly through the same code path the pipeline uses, on both the v2 and v3
models.

`audio/pipeline.rs` passed VAD a redemption time of 400 ms, the silence it waits for
before deciding an utterance ended. Pauses *inside* a sentence run 300-700 ms, so the
segmenter cut at nearly every one. Each fragment then reached the model without its
context and the words around the cuts were lost. The repository already documented the
problem in `audio/import.rs`: "400ms fragments speech at every natural sentence/topic
pause (500ms-2s)" — the batch path used 2000 ms while the live path stayed at 400 ms, and
a comment in `audio/vad.rs` claimed the pipeline passed 2000 ms when it did not.

Measured on a 42 s recording of ordinary speech, transcribing each VAD segment with
Parakeet:

```
 400 ms -> 5 segments, mean 1486 ms: "Mm-hmm. | I mean | Then | Maybe the"
 800 ms -> 1 segment,  mean 8620 ms: "But this is just millions of dollars per minute.
                                      Maybe they'll make you"
1200 ms -> identical to 800 ms
2000 ms -> identical to 800 ms
```

Fix: `PIPELINE_VAD_REDEMPTION_MS = 800`. 800 ms is the smallest value measured to keep
sentences intact, chosen over larger ones because the wait is added to every segment's
latency. Covered by `vad_redemption_bridges_pauses_inside_a_sentence`, and the measurement
itself is repeatable via `vad_redemption_time_changes_transcript_quality`.

Known trade-off: VAD has no maximum segment length, so uninterrupted speech produces one
long segment that is only emitted once the speaker pauses for 800 ms.

Unrelated observation, left alone: `audio/vad.rs` gates very short batch results on
`rms < 0.2 || peak < 0.20` while the comment above it describes 0.03/0.08. An RMS of 0.2
is far above ordinary speech. This is the import path, not live recording.

### Downloading the model

The corporate network's TLS interception blocks OCSP/CRL lookups, so downloads fail with
`CRYPT_E_NO_REVOCATION_CHECK` unless curl is passed `--ssl-no-revoke`. Chain and hostname
verification still apply. Base URL:
`https://huggingface.co/altunenes/parakeet-rs/resolve/main/nemotron-3.5-asr-streaming-0.6b-onnx`

## Build and packaging notes

- Run the app from `target/release/meetily.exe`. The debug binary points its webview at
  `devUrl` (`http://localhost:3118`) and shows `ERR_CONNECTION_REFUSED` unless
  `pnpm dev` is running; the release binary embeds the static export instead.
- `pnpm run tauri:build` auto-detects the GPU and adds `--features cuda`, which fails at
  the CMake configure step. Use the CPU variant.
- The `nsis` bundle target downloads its toolchain at build time and fails behind the
  corporate proxy with HTTP 407. Build with `--bundles msi`.
- `createUpdaterArtifacts: true` requires `TAURI_SIGNING_PRIVATE_KEY`, which belongs to
  the upstream project. Override it off for local builds with a `--config` file.

## Out of scope

- Integrating Nemotron 3.5 ASR or any other new model.
- Refactoring the pre-existing `[patch]`/`[profile]` keys in `frontend/src-tauri/Cargo.toml`
  that Cargo ignores because they are not declared at the workspace root.
- The archived Python backend under `backend/`.
