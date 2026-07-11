# Native Whisper Streaming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add low-latency native Whisper transcription with LocalAgreement-confirmed text and an ephemeral live preview without changing persistence or non-Whisper providers.

**Architecture:** The existing mixer and VAD emit incremental speech input plus a completed-utterance boundary. A Rust `WhisperStreamingSession` repeatedly decodes a bounded window through the existing `WhisperEngine`, confirms stable words across two hypotheses, emits preview separately, and routes only confirmed text through the existing persisted event.

**Tech Stack:** Rust 2021, Tokio, Tauri events, whisper-rs/whisper.cpp, Silero VAD, TypeScript, React 18, Next.js 14.

---

### Task 1: Define streaming inputs and LocalAgreement state

**Files:**
- Create: `frontend/src-tauri/src/audio/transcription/streaming.rs`
- Modify: `frontend/src-tauri/src/audio/transcription/mod.rs`

- [ ] **Step 1: Write failing unit tests** for two-pass prefix confirmation, punctuation-insensitive comparison, duplicate suppression, final flush, and a 15-second bounded audio window.
- [ ] **Step 2: Run `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer cargo test -p meetily audio::transcription::streaming::tests --lib`** and verify compilation fails because the streaming types do not exist.
- [ ] **Step 3: Implement minimal types:** `TranscriptionInput`, `StreamingWord`, `StreamingHypothesis`, `StreamingUpdate`, `LocalAgreement`, and `WhisperStreamingSession`.
- [ ] **Step 4: Re-run the targeted tests** and verify all new streaming tests pass.
- [ ] **Step 5: Run `cargo fmt --all -- --check`**, format if needed, then commit with `test(transcription): define streaming agreement behavior`.

### Task 2: Extract timestamped words from whisper-rs

**Files:**
- Modify: `frontend/src-tauri/src/whisper_engine/whisper_engine.rs`
- Test: unit tests colocated in `frontend/src-tauri/src/whisper_engine/whisper_engine.rs`

- [ ] **Step 1: Write failing tests** for grouping leading-space BPE tokens, attaching punctuation, skipping special tokens, and averaging token probability.
- [ ] **Step 2: Run the focused Whisper helper tests** and verify they fail for the missing grouping function.
- [ ] **Step 3: Implement `transcribe_streaming_window(audio, language, prompt)`** with a fresh state, explicit prompt, no internal context, token timestamps, quiet logging, and typed timestamped output.
- [ ] **Step 4: Re-run the helper and streaming tests** and verify they pass.
- [ ] **Step 5: Run Rust formatting and commit** with `feat(whisper): expose timestamped streaming hypotheses`.

### Task 3: Route active speech and utterance boundaries

**Files:**
- Modify: `frontend/src-tauri/src/audio/vad.rs`
- Modify: `frontend/src-tauri/src/audio/pipeline.rs`
- Modify: `frontend/src-tauri/src/audio/recording_manager.rs`
- Modify: `frontend/src-tauri/src/audio/transcription/worker.rs`

- [ ] **Step 1: Write failing tests** for VAD activity/time accessors and for provider routing rules: Whisper consumes streaming input, Parakeet ignores it, and both consume utterance end.
- [ ] **Step 2: Run the focused tests** and verify the expected missing APIs fail.
- [ ] **Step 3: Add read-only VAD accessors** for active speech and processed 16 kHz time.
- [ ] **Step 4: Change the transcription channel to `TranscriptionInput`** and emit resampled active-speech deltas plus complete VAD utterances.
- [ ] **Step 5: Integrate one `WhisperStreamingSession` into the serial worker**, emitting confirmed updates through `transcript-update` and preview through `transcript-preview`.
- [ ] **Step 6: Add final fallback:** if the streaming final decode fails, call the existing complete-chunk transcription path.
- [ ] **Step 7: Run focused tests, `cargo check -p meetily`, and formatting**, then commit with `feat(transcription): stream active whisper utterances`.

### Task 4: Add ephemeral preview state to the frontend

**Files:**
- Create: `frontend/src/lib/transcript-preview.ts`
- Create: `frontend/tests/lib/transcript-preview.test.mjs`
- Modify: `frontend/src/types/index.ts`
- Modify: `frontend/src/services/transcriptService.ts`
- Modify: `frontend/src/contexts/TranscriptContext.tsx`
- Modify: `frontend/src/app/_components/TranscriptPanel.tsx`
- Modify: `frontend/src/components/VirtualizedTranscriptView.tsx`

- [ ] **Step 1: Write a failing dependency-free Node test** showing preview replacement, whitespace clearing, and preservation outside persisted transcript arrays.
- [ ] **Step 2: Run `node frontend/tests/lib/transcript-preview.test.mjs`** and verify it fails because the reducer module is missing.
- [ ] **Step 3: Implement `TranscriptPreview` and its reducer**, add the Tauri listener, and expose optional preview state from context.
- [ ] **Step 4: Render preview after confirmed segments** with subdued styling and ensure the empty state does not hide a non-empty preview.
- [ ] **Step 5: Run the Node test, `pnpm exec tsc --noEmit`, and `pnpm run build`**, then commit with `feat(transcript): render ephemeral whisper preview`.

### Task 5: Documentation and full validation

**Files:**
- Modify: `README.md` or `docs/architecture.md` only if the public behavior is otherwise undocumented.
- Modify: PR template fields in the PR body, not repository files.

- [ ] **Step 1: Document** that local Whisper shows provisional text but persists only confirmed text.
- [ ] **Step 2: Run targeted Rust streaming tests** and confirm zero failures.
- [ ] **Step 3: Run `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer cargo test -p meetily --lib`** and record the exact result, including any unchanged baseline failure.
- [ ] **Step 4: Run `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer cargo check -p meetily`**, `cargo fmt --all -- --check`, the new Node test, TypeScript checking, and the Next production build.
- [ ] **Step 5: Review `git diff --check`, `git status -sb`, and the complete diff** against the design constraints; remove unrelated changes.
- [ ] **Step 6: Commit final documentation** with `docs(transcription): document native whisper streaming`.

### Task 6: Publish issue and draft PR

**Files:**
- No repository files unless review finds a correction.

- [ ] **Step 1: Authenticate GitHub CLI or use the connected GitHub app**, then search for an existing native Whisper streaming issue.
- [ ] **Step 2: Create an enhancement issue if none exists**, describing current independent VAD chunks, desired LocalAgreement behavior, and acceptance criteria.
- [ ] **Step 3: Push `feature/whisper-streaming` to the user's fork** with upstream tracking.
- [ ] **Step 4: Open a draft PR from the fork branch to `Zackriya-Solutions/meeting-minutes:devtest`**, link the issue with `Fixes #...`, and fill every `CONTRIBUTING.md` template section.
- [ ] **Step 5: Inspect PR checks** and report the branch, commits, PR URL, target, validation evidence, and any maintainer follow-up.
