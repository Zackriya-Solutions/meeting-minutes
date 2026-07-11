# Native Whisper Streaming Design

## Goal

Improve live local Whisper transcription by repeatedly decoding the active utterance, confirming stable words with LocalAgreement-2, and showing the remaining hypothesis as an ephemeral preview. Keep the existing `whisper-rs` models, accelerators, audio capture, persistence, and Parakeet behavior.

## Constraints

- Remain fully local and cross-platform.
- Add no Python, Torch, Swift, sidecar, or second model format.
- Do not import the experimental `nut_whisper.rs` implementation wholesale.
- Keep `transcript-update` as the only persisted transcript event.
- Never save provisional text to SQLite, recording history, or IndexedDB.
- Leave Parakeet on the existing VAD-finalized path.
- Keep the change narrow enough to review independently of unrelated audio refactors.

## Architecture

The existing mixer and Silero VAD remain authoritative for speech boundaries. While VAD reports an active utterance, the pipeline sends small speech-only audio deltas to the transcription worker. At the VAD boundary it sends the complete utterance as a finalization input.

```text
Mixed 48 kHz audio
        |
        v
ContinuousVadProcessor
   | active speech deltas       | completed utterance
   v                            v
StreamingAudio              UtteranceEnd
          \                  /
           WhisperStreamingSession
             | decode every 1 s
             | retain at most 15 s
             | LocalAgreement-2
             v
       +-------------------+
       |                   |
transcript-preview   transcript-update
 ephemeral UI         existing persistence
```

`TranscriptionInput` replaces the transcription channel's ambiguous `AudioChunk` payload:

- `StreamingAudio(AudioChunk)` carries incremental speech audio. Whisper consumes it; other providers ignore it.
- `UtteranceEnd(AudioChunk)` carries the complete VAD segment. Whisper uses it for final correction and flush; Parakeet processes it exactly as it processes today's chunks.

## Streaming session

`WhisperStreamingSession` is a small state machine owned by the single transcription worker. It has no Tauri or database dependency.

It maintains:

- buffered 16 kHz speech audio;
- absolute buffer start time;
- time of the most recent inference;
- previous unconfirmed word hypothesis;
- last confirmed word boundary;
- a bounded confirmed-text prompt.

The session asks for an inference after each additional second of speech. The inference window is bounded to the most recent 15 seconds. Confirmed words older than the window remain available only as a short prompt, which bounds memory and decode cost during unusually long uninterrupted speech.

## LocalAgreement-2

Whisper returns word-like units with absolute start/end timestamps and token probabilities. For every new hypothesis:

1. Drop words ending at or before the last confirmed boundary.
2. Compare normalized word text with the previous unconfirmed hypothesis.
3. Commit the longest common prefix seen in both consecutive hypotheses.
4. Emit the unmatched suffix as preview.
5. On `UtteranceEnd`, run one authoritative decode of the full VAD segment and commit every remaining word.

Comparison is case-insensitive and ignores surrounding punctuation for matching, while emitted text preserves Whisper's original spelling and punctuation. A small timestamp tolerance prevents overlapping windows from duplicating already confirmed words.

## Whisper inference

The existing one-shot transcription API remains unchanged for import, retranscription, and Parakeet-compatible flows. A new `transcribe_streaming_window` method:

- creates a fresh Whisper state;
- enables token timestamps;
- disables internal previous-text conditioning;
- applies the bounded confirmed prompt explicitly;
- extracts token text, timestamps, and probability;
- groups leading-space BPE pieces into word-like units;
- returns a typed `StreamingHypothesis`.

Keeping this separate prevents changes to the established offline transcription behavior.

## Events and persistence

Confirmed output uses the existing `TranscriptUpdate` structure with `is_partial = false`. Therefore the existing recording listener, JSON transcript history, IndexedDB write, ordering, and meeting reload paths remain intact.

Preview output uses a new event:

```text
transcript-preview {
  text,
  audio_start_time,
  audio_end_time
}
```

An empty preview clears the UI. Preview events have no sequence ID and are never observed by persistence listeners.

## Frontend

`TranscriptContext` owns one optional `TranscriptPreview` beside the confirmed transcript array. The service subscribes to `transcript-preview`, replacing the previous preview atomically. Confirmed transcripts continue through the existing buffer.

`VirtualizedTranscriptView` accepts an optional preview and renders it after confirmed segments with subdued color and a pulse marker. Meeting details never pass a preview, so historical views remain unchanged. Starting or stopping a recording clears the preview.

## Error handling

- Streaming inference failures emit the existing non-fatal `transcription-warning` and retain buffered audio for the next attempt.
- A final inference failure falls back to the existing one-shot transcription of the complete VAD segment.
- Empty or low-confidence hypotheses clear preview without creating persisted segments.
- Channel closure finalizes any session state before the worker exits.
- Non-Whisper providers ignore `StreamingAudio` and process `UtteranceEnd` normally.

## Verification

Pure Rust unit tests cover word grouping, LocalAgreement confirmation, duplicate suppression, preview replacement, bounded audio windows, and final flush. Existing worker tests verify input routing where possible without loading a model.

A dependency-free Node test covers preview state replacement and clearing. TypeScript compilation and Next build verify component integration. Rust formatting, targeted tests, full library tests, and `cargo check` verify backend integration; the pre-existing floating-point timeout test is recorded separately if it remains the only full-suite failure.

## Out of scope

- Replacing Whisper with WhisperKit, SimulStreaming, sherpa-onnx, or `whisper-stream-rs`.
- Changing model downloads or model selection.
- Speaker diarization.
- Persisting or replaying provisional text.
- Refactoring the duplicated recording start paths.
