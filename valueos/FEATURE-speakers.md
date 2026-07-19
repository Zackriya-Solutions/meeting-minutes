# Feature: Speaker labelling

Goal: label who is speaking in the transcript (at minimum **Me** vs **Other**), using only
what the existing transcription stack supports — **no new diarization model, no upstream
edits**.

## Achieved level: (c) — documented, helper pre-positioned, inert today

Per the agreed decision we did **not** edit upstream. This document records exactly what the
stack can and cannot do, and wires a label-mapping helper in our namespace that becomes live
the moment an upstream change provides a per-segment source.

## What the stack actually does (investigated, read-only)

- **Capture is per-source.** Mic and system audio are captured as two independent streams,
  each tagged with a `DeviceType` (`Microphone` / `System`):
  `frontend/src-tauri/src/audio/stream.rs:388` (mic) and `:406` (system);
  `audio/recording_state.rs:12,24` (the `DeviceType` enum, carried on every `AudioChunk`).
- **…then irreversibly mixed to mono before transcription.** The pipeline routes chunks by
  device type into `mic_buffer` / `system_buffer` (`audio/pipeline.rs:60-63`) and **sums them
  into a single mono window** (`ProfessionalAudioMixer::mix_window`, `pipeline.rs:154-189`,
  called at `:826`). Immediately after, the per-source tag is **overwritten** to
  `DeviceType::Microphone` (`pipeline.rs:849`, `:875`, comment "Mixed audio"). Only one mono
  stream reaches VAD → transcription.
- **The engine emits no speaker signal.** `TranscriptResult` is `{ text, confidence,
  is_partial }` only (`audio/transcription/provider.rs:42-46`); neither Whisper
  (`whisper_engine/whisper_engine.rs`) nor Parakeet (`parakeet_provider.rs`) produces speaker
  turns.
- **The `source` field that reaches us is a constant.** The live event `TranscriptUpdate` has
  a `source: String` (`audio/transcription/worker.rs:30`) but it is hardcoded
  `"Audio"` (`worker.rs:211`). The frontend `Transcript` object
  (`frontend/src/types/index.ts`) does not even carry `source` — `TranscriptContext.addTranscript`
  drops it.

**Conclusion:** neither "Me/Other" (level a) nor engine diarization (level b) is achievable
without upstream changes. Level a needs the capture `device_type` preserved through the mix →
VAD → worker → `TranscriptUpdate.source` (and onto the frontend `Transcript`); level b needs a
diarization model (explicitly out of scope). Hence **level (c)**.

## What we shipped (our namespace only)

- `frontend/src/valueos/capture/speakerLabels.ts` — `sourceToLabel(source)` maps
  mic→`Me`, system/received→`Other`, `speaker N`→`Speaker N`, and **everything else (incl. the
  current `"Audio"`) → null (no label)**. `labelTranscript(segments)` joins segment text,
  prefixing `"<label>: "` only when a label resolves.
- `frontend/src/valueos/capture/useRecordingController.ts` — the single text-join point
  (`transcriptText`, `stop()`, and the confirmed live span) now goes through
  `labelTranscript`. This is the **one seam** that feeds the live view, the stored `.txt`, and
  the uploaded `raw_content`, so labels will appear in all three at once when they become real.
- Because segments carry no source today, output is **byte-for-byte identical to a plain
  newline join** — verified by tests. Nothing is over-promised in the UI.

## To reach level (a) later (the smallest upstream change — NOT done here)

1. `audio/pipeline.rs` — stop discarding the source: transcribe mic and system as two tagged
   passes (or thread `device_type` through the VAD segment) instead of the single mixed pass
   that overwrites the tag at `pipeline.rs:849`.
2. `audio/transcription/worker.rs:211` — set `source` from that `device_type` instead of the
   `"Audio"` constant.
3. Carry `source` to the webview: add it to `Transcript` (`frontend/src/types/index.ts`) and
   copy it in `TranscriptContext.addTranscript` — **or**, to keep the upstream footprint
   smaller, have a valueos-side listener subscribe to the `transcript-update` event directly
   (it already carries `source`) and maintain our own labelled buffer.

Once any of these populates a real `source`, `labelTranscript` produces `Me:` / `Other:`
prefixes automatically, in the stored file and the upload, with zero further changes here.

## Tests

`valueos/shell-tests/speaker-labels.test.ts` — mapping (`sourceToLabel`) and the join
(`labelTranscript`): proves it's a no-op plain join for the current no-source / `"Audio"`
segments (no regression) and that it produces correct `Me:` / `Other:` / `Speaker N:` prefixes
the moment a real source is present.
