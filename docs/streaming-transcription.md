# Streaming Transcription (self-hosted realtime ASR)

Meetily's built-in transcription is **one-shot per chunk**: the audio pipeline
applies voice activity detection, cuts the speech into segments, and asks the
provider to transcribe each segment on its own. That is the right shape for
Whisper and Parakeet, and it is what every local provider uses.

Some ASR models are built the other way around. They expect a **persistent
connection**, a continuous stream of audio, and they do their own segmentation
server-side — emitting partial hypotheses that get revised as more audio arrives.
Mistral's `Voxtral-Mini-Realtime` served by vLLM is the worked example.

The **Custom Realtime (WebSocket)** provider covers that second shape. It is
entirely optional: if you don't configure it, nothing about local transcription
changes.

> This connects Meetily to a server **you run**. Audio leaves the app over the
> network to whatever endpoint you configure, so point it at your own machine or
> your own infrastructure — not a third party you don't control.

## Configuring it

Settings → **Transcription** → provider **Custom Realtime (WebSocket)**:

| Field | Meaning |
| --- | --- |
| **Endpoint** | `ws://host:port`, `wss://…`, or a full path. A bare host gets `/v1/realtime` appended; `http(s)://` is rewritten to `ws(s)://`. |
| **Model** | Model id the server should load, e.g. `voxtral-mini-transcribe-realtime-2602`. Validated server-side — a wrong name fails the connection test. |
| **API key** | Optional. Sent as `Authorization: Bearer …`. Leave empty for a server with no auth. |

**Test Connection** opens the socket and performs the handshake (which validates
the model) before you commit the settings, so a typo surfaces here rather than at
the start of a meeting.

Language selection is disabled for this provider: the server detects the language
itself, the same way Parakeet does.

## Running a Voxtral Realtime endpoint

```bash
vllm serve mistralai/Voxtral-Mini-4B-Realtime-2602 \
  --tokenizer_mode mistral --config_format mistral --load_format mistral
```

Then point Meetily at `ws://localhost:8000`. Transcription delay is a
**server-side** knob (the model's `tekken.json`, 80–1200 ms in 80 ms steps;
Mistral recommends 480), not something the client negotiates per session.

## What happens during a recording

The pipeline gains a second tap that carries the **continuous, pre-VAD** mixed
audio — the same mix that gets recorded, before speech segmentation. That stream
is resampled to 16 kHz mono and pushed up the socket; the VAD chunk path is left
running but its output is discarded, since the server segments for itself.

Results come back as a growing transcript. Meetily splits it into sentences: the
in-progress sentence updates in place as you speak, and settles into the
transcript when it completes. Everything is emitted on the same `transcript-update`
event the local providers use, so transcript history, persistence, reload
recovery, and summarization all work unchanged.

**If the connection drops mid-recording**, the provider closes out the text it
had, reconnects with backoff (3 attempts), and carries on. Audio from the outage
window is discarded rather than replayed, so the live transcript doesn't fall
permanently behind. If all attempts fail you get an explicit error — transcription
stops for the rest of that recording, but **the recording itself keeps going**, so
the audio can be transcribed afterwards.

## The wire contract

The provider abstraction is **not** Voxtral-specific. A streaming provider only
has to satisfy the `StreamingTranscriptionProvider` trait
(`src-tauri/src/audio/transcription/streaming_provider.rs`):

- accept 16 kHz mono `f32` audio frames pushed at it for the life of a recording
- emit `Partial { text }` for interim hypotheses and `Final { text, confidence }`
  for settled ones — both carrying the **cumulative** text of the current utterance
- emit `Error { message, fatal }`, where `fatal` means transcription has ended
- verify itself on demand for the "Test Connection" button

The persisted config carries a `protocol` discriminator (default
`voxtral-realtime`). Adding another dialect — Deepgram live, a Whisper streaming
server, your own — is a new module plus one match arm in
`build_streaming_provider`; no changes to the pipeline, worker, events, or UI.

The `voxtral-realtime` dialect itself, verified against a live vLLM endpoint:

```text
 connect ws(s)://{host}/v1/realtime            (wss:// adds Authorization: Bearer)
 → client: {"type":"session.update","model":…}            (model REQUIRED, flat)
 → client: {"type":"input_audio_buffer.commit"}           (REQUIRED — opens the buffer)
 ← server: {"type":"session.created","id":…}              (ignored; may arrive late)
 → client: {"type":"input_audio_buffer.append","audio":"<b64 PCM16>"}  (repeated)
 → client: {"type":"input_audio_buffer.commit","final":true}          (on finish)
 ← server: {"type":"transcription.delta","delta":…}       (INCREMENTAL, not cumulative)
 ← server: {"type":"transcription.done","text":…}
 ← server: {"type":"error","error":…}
```

Audio frames are base64-encoded 16 kHz mono PCM16-LE. Two things that are easy to
get wrong and cost real debugging time:

- **The leading `commit` is required.** Omit it and the server ingests every
  append but emits no delta and no `done` — the session just hangs silently.
- **`session.update` accepts only `model`**, top-level. Unknown fields are
  silently ignored, so `language` and delay settings are *not* part of this
  contract.

## Troubleshooting

| Symptom | Likely cause |
| --- | --- |
| Test Connection fails with `model_not_found` | Model id doesn't match what the server loaded. |
| Test Connection fails with a 502 | Endpoint reachable but the ASR backend is still booting. |
| Recording starts, transcript stays empty | Server accepted audio but never sent a delta — usually a dialect mismatch, not a Meetily bug. Check the server log. |
| "Reconnecting" warnings during a meeting | Server restarted or the network blipped; transcription resumes on its own. |
