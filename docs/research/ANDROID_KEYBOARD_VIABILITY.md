# PulseTalk Android Keyboard (IME) Viability Research

Date: 2026-09-03. Research compiled from four parallel investigations: Android IME landscape, Parakeet on Android, desktop-mobile sync, and a survey of the existing PulseTalk Rust codebase.

## Verdict

Viable, with a clear architecture. Every hard requirement has a proven precedent: shipping open-source keyboards already run local ASR via sherpa-onnx on Android, at least one (Dictus) already ships Parakeet specifically, and our desktop Parakeet code is largely portable Rust. The two real constraints are: Parakeet has no true streaming (chunked pseudo-streaming only), and the ~650 MB INT8 model must live in a separate service process, not the IME process.

## 1. Android IME landscape

- `InputMethodService` has no special hard memory quota, but IME processes are aggressively reclaimed by the low-memory killer when not foregrounded. Large model loads inside the IME process risk OOM kills on low-RAM devices. Android 17 is adding stricter per-app memory shielding.
- Mic access from an IME is allowed under standard `RECORD_AUDIO` rules (no IME-specific restriction beyond the Android 12+ global mic toggle and in-use indicator), but production keyboards typically delegate capture to a companion activity or service to reduce friction and review scrutiny.
- Enabling a third-party keyboard triggers the OS "can collect everything you type" warning. Play Store adds Data Safety declarations and in-app prominent disclosure requirements. An offline-first design with no INTERNET permission on the IME (or very deliberate use of it) materially de-risks review and user trust. Note: model downloads and sync need network, so permission strategy needs care (e.g., separate process, clear disclosure).
- Foundation codebases:
  - **HeliBoard** (GPL-3.0, Kotlin, active, de-facto OpenBoard successor): best typing-engine base, but GPL-3.0 copyleft forces source publication of derivatives. Decision needed vs. PulseTalk licensing plans.
  - **FlorisBoard** (Apache-2.0, slower-moving, NLP engine rewrite in limbo): permissive license but higher maintenance risk.
  - **FUTO Keyboard / Voice Input** (Source First License, NOT open source, restricts commercial use): great reference implementation, not a legal base for us.
  - **OpenBoard**: dead.
- Building autocorrect/prediction from scratch is a multi-month effort on its own; reuse a base keyboard rather than building typing from zero.

## 2. Parakeet on-device (Android)

- Parakeet TDT 0.6B v2 (EN) / v3 (25 languages) are CC-BY-4.0, commercial-friendly. INT8: encoder ~652 MB, decoder ~18 MB. This matches the exact models and HuggingFace repos our desktop app already uses (`istupakov/parakeet-tdt-0.6b-*-onnx`).
- **sherpa-onnx** is the dominant Android runtime, with prebuilt Parakeet "simulated streaming" demo APKs. Our desktop code uses the `ort` crate directly instead; `ort` also supports Android (ONNX Runtime Mobile, NNAPI EP), so we can port our own engine rather than adopting sherpa-onnx.
- Parakeet TDT is architecturally offline. "Real-time" UX is achieved by chunked re-decoding with overlap (a few-second windows), same as sherpa-onnx's simulated streaming. Our desktop dictation is already single-shot batch, so this matches.
- Shipping precedents: **Dictus** (Android IME with Parakeet via sherpa-onnx), **WhisperTypeKeyboard** (Whisper via sherpa-onnx in an IME), **FUTO Voice Input** (whisper.cpp). No public phone-specific Parakeet RTF/RAM numbers found; sherpa-onnx was measured ~51x faster than whisper.cpp for the same model on Android, a good proxy that ONNX-runtime Parakeet is practical on modern phones.
- Realistic device floor: flagship/upper-midrange 2024+ phones (8 GB+ RAM) for Parakeet INT8. For budget devices, offer a fallback engine (Moonshine, which has true streaming and sub-100 MB models, or whisper tiny/base).
- Architecture: host the model in a bound/foreground service in a separate process (`android:process`), IME talks to it via IPC. Keeps the keyboard alive if the model process is reclaimed.

## 3. Desktop-mobile sync

Recommended stack (privacy-first, local-first, one Rust implementation):

1. **Shared Rust core crate** (`pulsetalk-core`): storage model, sync logic, ASR interface. Consumed directly by the Tauri desktop backend and exposed to Kotlin via **uniffi** (production-proven, pre-1.0, pin versions). Cross-compile with `cargo-ndk`.
2. **Data model**: Automerge CRDT documents per meeting/transcript and for dictation vocab/settings. Automerge 2.x is Rust-native and fast.
3. **Transport**: **iroh** (1.0 stable, June 2026): QUIC, dial-by-public-key, mDNS/LAN discovery, NAT hole punching, self-hostable relays as fallback. There is a documented Automerge-over-iroh pattern with a shipping example app (outl).
4. **Optional opt-in cloud fallback**: encrypted blob mailbox (self-hosted small service or user-controlled S3 bucket) storing only CRDT ciphertext, key derived from QR-code device pairing. Off by default to preserve the local-only story.

Rejected: PowerSync/ElectricSQL (cloud-Postgres-centric), embedding Syncthing (file-oriented, heavy), Yjs (JS-first).

## 4. What we already have (repo survey)

- `frontend/src-tauri/src/parakeet_engine/model.rs` (`ParakeetModel`): pure `ort` + `ndarray` batch inference (preprocessor, encoder, TDT greedy decode with timestamps). No Tauri/OS deps. **Most directly portable piece.** Needs an execution-provider swap from CPU-only for Android (NNAPI/XNNPACK).
- `audio/transcription/provider.rs` `TranscriptionProvider` trait: clean async abstraction over Whisper/Parakeet, reusable as the core ASR interface.
- `dictation/session.rs` state machine (Idle → Listening → Transcribing → Cleaning → Delivering/Failed): pure logic, portable. The Windows dictation flow (hold hotkey, capture, one batch transcribe, cleanup, paste) maps 1:1 to a keyboard mic-button flow.
- Not portable as-is: `ParakeetEngine` wrapper (desktop path assumptions), `dictation/coordinator` + `short_audio` (cpal, Windows delivery), `database/` (sqlx + Tauri AppHandle). Android supplies AudioRecord capture and its own storage; persistence would move to the shared core with the Automerge model.

## 5. Recommended phased plan

- **Phase 0, spike (1-2 weeks)**: extract `ParakeetModel` + `TranscriptionProvider` into a `pulsetalk-core` crate, cross-compile for `aarch64-linux-android`, wrap with uniffi, and benchmark INT8 v3 on 2 or 3 real phones (RTF, RAM, chunked latency). This is the single load-bearing unknown (no public phone benchmarks exist), so it gates everything.
- **Phase 1, voice input app**: standalone Android voice-input app (registers as a voice IME / RecognizerIntent target usable from Gboard and HeliBoard), model in a separate-process foreground service, chunked pseudo-streaming UI. Much smaller scope than a full keyboard, validates the ASR stack with users.
- **Phase 2, full keyboard**: fork HeliBoard (accepting GPL-3.0 for the keyboard app) or build a minimal keyboard if licensing rules that out, integrating the Phase 1 voice service natively plus PulseTalk theming and vocab.
- **Phase 3, sync**: Automerge + iroh in `pulsetalk-core`, QR pairing, sync dictation vocab/settings first, then meeting transcripts/summaries.

## 6. Key risks

| Risk | Mitigation |
|---|---|
| No phone-verified Parakeet performance numbers | Phase 0 spike before committing |
| 650 MB model footprint and RAM on mid-range devices | Separate-process service, Moonshine/whisper-tiny fallback engine behind the existing provider trait |
| No true streaming (chunked pseudo-streaming) | Matches current desktop UX; set product expectations accordingly |
| HeliBoard GPL-3.0 copyleft vs. product licensing | Decide early; FlorisBoard (Apache-2.0) or in-house minimal keyboard are the alternatives |
| Keyboard trust/Play review scrutiny | Offline-by-default, prominent disclosure, network confined to model download and opt-in sync |
| iroh mobile and uniffi pre-1.0 maturity | Pin versions, sync is Phase 3 so it does not block the keyboard |
