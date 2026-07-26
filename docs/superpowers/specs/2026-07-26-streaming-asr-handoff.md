# Handoff: streaming transcription and model selection

**Date:** 2026-07-26
**Branch:** `feat/gpu-whisper-live-transcription`
**Repo:** `C:\Work\meetily`

This is a handoff for a fresh session. It records what was measured, what was
fixed, what is still broken, and what must not be re-litigated.

---

## 1. The problem to solve

The user plays a YouTube video through system audio and expects a live
transcript. Two failures, in priority order:

**A. Nothing is committed while someone keeps talking.** With a fast, continuous
speaker the transcript sits still for the whole recording and the text only
lands after Stop. Observed directly: at 00:56 into a recording there was exactly
one committed segment, timestamped 00:05, while the live preview underneath kept
producing new text.

This is not a bug that crept in — it is a documented trade-off. `audio/vad.rs`
carries a comment recording that a maximum segment length was tried and reverted
because cutting an utterance at an arbitrary point cost more accuracy than the
latency was worth. Segments are therefore bounded only by the speaker's own
pauses, and `PIPELINE_VAD_REDEMPTION_MS = 800` means an 800 ms pause is required.
A presenter who does not pause produces one enormous segment.

**The user has now rejected that trade-off.** Text must arrive continuously.
Re-proposing "wait for a pause" is not an answer.

**B. Accuracy is poor and inconsistent, and it is worst on fast speech.** The
user reports `large-v3` performing badly "for no reason" and finds the
already-integrated Nemotron 3.5 better.

### Acceptance criterion, in the user's words

> "https://www.youtube.com/watch?v=ev0VPI7Zgh8 kalite esansin bu video bu videoyu
> eksiksiz anlamali en azindan 1 dk sini"

The first 60 seconds of that video must be transcribed completely and correctly,
with text appearing continuously during playback rather than after Stop.

---

## 2. What was measured (do not redo this)

A replay harness exists and runs in seconds without a microphone:

```bash
ffmpeg -i audio.mp4 -ac 1 -ar 48000 -f f32le case.raw
cd frontend/src-tauri
MEETILY_VAD_CASE=case.raw cargo test --features cuda --lib \
  vad_coverage_on_a_real_recording -- --nocapture --ignored
```

`ffmpeg` is at `frontend/src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe`.
Recordings live under
`C:\Users\DOGTERZ\OneDrive - Mercedes-Benz (corpdir.onmicrosoft.com)\Music\meetily-recordings\`;
each meeting folder holds `audio.mp4`, `transcripts.json` and `metadata.json`.
`audio.mp4` is byte-for-byte the buffer VAD sees, because `pipeline.rs` hands the
same mixed window to both the recorder and the detector.

Findings on a real 172 s recording:

| Question | Answer |
|---|---|
| Does the pipeline lose audio before VAD? | No. Zero-padding is 0.1% of the saved file; all channels are unbounded, so nothing is dropped for backpressure. |
| Live vs offline VAD coverage | 63% live, 58% offline — the same. VAD, not the plumbing, was the bottleneck. |
| Coverage after the fixes below | 67%, every recovered span verified to be speech |
| Are the gaps real silence? | Mostly yes (median 0.0069 RMS, spectral flatness 0.29). But seconds 95-98 had speech-band energy 0.72-0.94 and flatness 0.010-0.019 — unmistakable speech, entirely missed. |
| Does raising the input level fix it? | Barely: 8x gain moved coverage 58% -> 67%. Level is a contributing factor, not the cause. |
| Speech vs noise discriminator that works | Autocorrelation peak over the 80-400 Hz pitch range: speech 0.26-0.73, room noise 0.04-0.13. RMS cannot separate them (0.0145 speech vs 0.0118 noise). |

---

## 3. What was already fixed (committed, tested)

| Commit | Change |
|---|---|
| `1a34886` | Band-limited sinc resampler for the VAD feed (was a 5-tap moving average, ~-14 dB at Nyquist); AGC'd detector copy while the transcriber gets untouched audio from a separate history buffer; mixing window 600 ms -> 100 ms; a continuity ledger that recovers spans VAD never flagged, gated on energy *and* pitch; VAD errors no longer delete a window; inputs sanitised before Silero (whose own range check is `#[cfg(debug_assertions)]` only). |
| `f3d8697` | Utterances longer than the 60 s history buffer keep their opening. |
| `4b79c2c` | whisper.cpp refuses input under 1000 ms and returns nothing; the pipeline forwarded segments down to 50 ms, so one-word answers and short sentence openings silently vanished. Short audio is now padded with trailing silence. |

`cargo test --features cuda --lib` — 240 pass, 0 fail.

---

## 4. Dead ends — do not spend time here again

- **Ring buffer shredding audio with zero-padding.** Falsified: 0.1% of a saved file.
- **Backpressure dropping segments.** Falsified: every channel is `mpsc::unbounded`.
- **A segment-length cap inside silero-rs.** No such thing; two ~29.4 s segments were coincidence.
- **Segment ids starting at 2 or 3 meaning lost audio.** False. `SEQUENCE_COUNTER`
  in `audio/transcription/worker.rs:16` is a process-global that is never reset
  between recordings — the old `lib_old_complex.rs` did reset it. Cosmetic, but
  worth fixing while nearby.
- **Model size being the accuracy limit.** `large-v3` is already loaded, on GPU.

---

## 5. Leads that are open

**Rolling context is not carried between segments.** `set_initial_prompt` is used
only for the static user glossary (`whisper_engine.rs:629` and `:762`). Each VAD
segment is decoded blind, with no memory of the words before it. For streaming
ASR this is a leading cause of both mishearing and inconsistency, and it is
cheap to try.

**Whisper is an offline 30-second-window model.** Making it stream at all
requires something like LocalAgreement-2 (run overlapping windows, commit the
longest common prefix of two consecutive hypotheses). That is a real technique
with published results, but it is a workaround for using the wrong tool.

**A streaming-native model may be the correct answer.** NVIDIA's cache-aware
streaming FastConformer / Parakeet family is built for exactly this shape of
problem, and the repo already has `parakeet_engine/` plus a `nemotron-helper`
sidecar, so the integration surface exists.

---

## 6. Task for the new session

### 6.1 Research and choose a model

The user asked, verbatim:

> "bana suan benim gpu ile uyumlu en kaliteli en hizli calisabilecek speech to
> text modelini de locale indir ayni nemotron gibi"

Hardware: **NVIDIA RTX 5000 Ada Generation Laptop GPU**, compute capability 8.9,
driver 582.41, CUDA 13.3 toolkit available. Ada supports BF16 and FP8.

Research current options and recommend one, with reasoning the user can check.
Candidates worth pricing out — verify each against current sources rather than
trusting this list:

- NVIDIA Parakeet TDT (0.6B) and the cache-aware **streaming** FastConformer variants
- NVIDIA Canary / Canary-Qwen
- Nemotron speech models (already integrated via sidecar — the user prefers its output today)
- `whisper-large-v3-turbo` and distil-whisper variants, which trade a little accuracy for large speed gains over `large-v3`

Judge candidates on: **streaming capability first** (can it emit stable text
mid-utterance?), then word error rate, then real-time factor on this GPU, then
VRAM. A model that cannot stream does not solve problem A no matter how accurate
it is.

Then download it locally, following the existing pattern (`parakeet_engine/`,
`nemotron-helper/`, models under `%APPDATA%\com.meetily.ai\models\`).

### 6.2 Make the transcript arrive continuously

Design and implement continuous commit during long utterances. The existing
`transcript-partial` event already carries a live preview (the italic text in the
UI), so the UI plumbing for incremental text exists — what is missing is
*finalising* text without waiting for a pause.

Whatever the approach, it must not reintroduce the failure that caused the
maximum-segment-length idea to be reverted: cutting mid-utterance and losing the
words around the cut.

### 6.3 Prove it

Build a repeatable benchmark before tuning anything:

1. Capture the first 60 s of the reference video as a fixed test case.
2. Write down the reference transcript.
3. Extend the existing harness to run the real ASR over it and report **word
   error rate** and **real-time factor**, plus **time-to-first-committed-word**
   and **maximum gap between committed segments** — the last two are what
   problem A is actually about.

Then iterate against those numbers instead of re-recording by hand.

---

## 7. Environment notes

**CUDA is working but only through project-local configuration.** Six separate
problems had to be solved; all of the fixes live in `.cargo/`, which is
deliberately untracked and machine-specific:

1. The CUDA 13.3 installer placed Visual Studio build customizations but never
   set `CUDA_PATH_V13_3`, which `CUDA 13.3.Version.props` derives `CudaToolkitDir`
   from — MSBuild computed an empty directory.
2. Cargo's `[env]` does not override variables that already exist, and this
   machine ships `CUDA_PATH` pointing at 13.0. Every entry needs `force = true`.
3. ggml picks `52;61;70;75` when `CMAKE_CUDA_ARCHITECTURES` is undefined; CUDA 13
   dropped Maxwell, so nvcc rejects `compute_52`. Pinned to `89`.
4. A toolchain file is the only place early enough to set that — `CUDAARCHS` does
   not work, because ggml assigns the variable before `enable_language(CUDA)`
   ever reads it.
5. CUDA 13.3's CCCL headers require MSVC's conforming preprocessor
   (`-Xcompiler=/Zc:preprocessor`).
6. Its CUB requires C++17 (`CMAKE_CUDA_STANDARD 17` plus `/Zc:__cplusplus`).

After editing `.cargo/cuda-toolchain.cmake`, delete
`target/debug/build/whisper-rs-sys-*/out` — CMake only reads a toolchain file on
a fresh configure.

Run the app with `pnpm run tauri:dev:cuda` from `frontend/`. Startup takes a
while: the first route compile is ~7 s and `large-v3` has to load onto the GPU.
A blank window during that period is not a hang.

**A `GateGuard` hook denies the first write to each new file and the first edit
to each existing one, plus every destructive shell command.** It expects a short
statement of callers, affected API, data schemas and the user's verbatim
instruction, then accepts on retry. It can be disabled with `ECC_GATEGUARD=off`
or by adding `pre:edit-write:gateguard-fact-force` to `ECC_DISABLED_HOOKS` —
worth asking the user first, since it is their hook.

**Cost.** The session that produced this handoff ran past $450, much of it on
repeated CUDA rebuilds. Prefer the offline harness over full rebuilds, and batch
verification.
