# Feature: Live in-progress (interim) transcription signal

**Problem:** during capture only finalized segments appeared, so while the user spoke a long
sentence nothing showed until it committed — the UI looked frozen.

## Which path — investigation result

**Path (b): the engine has NO true interim stream.** The audio pipeline runs VAD on the mixed
audio and only sends a segment to the engine at **end-of-speech** (VAD redemption ~400 ms), so
there is one transcript update **per completed utterance** — nothing during a long sentence:

- `audio/pipeline.rs` — VAD emits complete speech segments; each becomes one transcription chunk.
- `audio/transcription/parakeet_provider.rs` — Parakeet (the active engine) sets
  `is_partial: false` **always** (no partials at all).
- `whisper_engine/whisper_engine.rs` — Whisper sets `is_partial = duration < 15s`, i.e. a
  **duration flag, not a mid-utterance interim**.

There is also an `audio-levels` event, but the monitor emits **fake sine-wave data** and isn't
started during capture — so it's not a usable real signal.

Per the prompt, we do **not fake words**. Instead we provide an unmistakable **real live
signal**, and keep a tested interim model ready for the day the engine gains a true stream.

## What we ship

1. **Real VU meter** (`useMicLevel.ts`) — from the webview's **own** microphone
   (`getUserMedia` + Web Audio `AnalyserNode`), independent of the Rust capture. The bars move
   with the user's actual voice — a genuine "it's hearing you" signal. Updates throttled to
   **~15 Hz**. If the mic is unavailable (no permission / unsupported), it degrades to the
   existing animated waveform. **Display-only** — this audio is never recorded, saved, or
   uploaded, and the stream + AudioContext are released on stop/unmount.
2. **Activity indicator** (`recognitionActivity` + the Recording screen's `ActivityChip`) — a
   pulsing **Listening… / Recognizing…** state (recognizing while the user is speaking or an
   interim is in flight), plus **"N s since last line"** which counts up during a long sentence
   and resets when a new line commits. This is derived from real state (mic level + commit
   timing), not decoration.
3. **Interim text, when it exists** (`liveTranscript.ts`) — the model already renders a trailing
   in-progress hypothesis as faded/italic with a blinking caret. With the current engine there is
   none (Parakeet emits only finals), so this is inert today but lights up automatically if a
   real interim stream is wired.

## Interim state model (`liveTranscript.ts`) — tested

- `reduceLive(state, update)` — the **event-stream** reducer: an INTERIM update replaces the
  single interim buffer (**latest wins, never appended**); a FINAL commits exactly once
  (dedup/replace by `sequence_id`) and **clears the interim**. This is the forward-compatible
  path — wiring a true interim stream here needs zero UI changes.
- `deriveLive(segments)` — the **snapshot** derivation used at runtime today (over the segment
  array): a trailing in-progress segment is the interim; the rest is committed. Fed into the
  controller's confirmed/partial split.

## Guarantees

- **Interim is DISPLAY-ONLY.** Only committed (final) segments flow into the enriched export and
  the `POST …/calls` upload — `transcriptFormat.ts` drops `is_partial` segments (tested: interim
  text is absent from the formatted output). Speaker labels + timestamps on finalized lines are
  preserved (WS1/WS2).
- **Clean end-of-capture.** `stop()` formats only the finalized segments (`transcriptsRef`); any
  dangling interim is never exported, and the VU meter's mic stream is released.

## Throttle

VU meter: ~15 Hz (`useMicLevel`). If a true interim text stream is later wired through
`reduceLive`, throttle its UI updates to ~5–10 Hz the same way.

## Tests (`live-transcript.test.ts`)

- Interim stream: latest interim replaces prior (no append); finalize commits once + clears
  interim; a duplicate final (same sequence_id) doesn't duplicate; a full two-utterance stream.
- `deriveLive` snapshot behavior.
- **Interim never exported** (formatTranscript excludes `is_partial`).
- `recognitionActivity` states; `rmsFromTimeDomain` level.
- `shell.test.tsx`: the activity indicator is present during capture (and "recognizing" with an
  in-flight interim).

## Upstream

**No upstream files modified.** The VU meter uses the webview's own `getUserMedia` (our
namespace); the activity/interim logic is all under `frontend/src/valueos/`. (A *true* interim
stream would require an upstream pipeline change — streaming partial audio to the engine before
end-of-speech — which is out of scope here and noted for a future decision.)
