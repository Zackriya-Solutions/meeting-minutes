# Transcription Providers

Meetily supports multiple transcription providers. Choose the one that best fits your hardware, privacy requirements, and feature needs.

## Existing Providers

### localWhisper

Runs Whisper models locally using `whisper.cpp`. Works on CPU or GPU (CUDA/Vulkan). No audio leaves your machine. Model selection is available at Settings → Transcription.

**GPU acceleration:** Automatic when building with CUDA/Vulkan support.

**Default:** Yes (`large-v3` multilingual model).

### parakeet

NVIDIA's Parakeet model via ONNX runtime. Fast transcription on CPU/GPU, optimized for English. Language selection is not supported. Model selection available at Settings → Transcription.

## Remote HTTPS (new)

Posts audio to a user-configured HTTPS endpoint. Use this to integrate with any WhisperX-compatible backend (self-hosted, cloud GPU, or custom inference server).

### Configuration

| Field | Description |
|-------|-------------|
| `endpoint_url` | HTTPS URL that accepts the contract below (required) |
| `bearer_token` | Token sent as `Authorization: Bearer <token>` (optional) |
| `model` | Model identifier forwarded to the worker (optional) |
| `default_lang` | Language hint, e.g. `en`, `ar` (optional) |
| `min_speakers` | Minimum speakers for diarization (optional) |
| `max_speakers` | Maximum speakers for diarization (optional) |

Configure via Settings → Transcription → "Remote HTTPS ASR" after setting up provider/UI in PR 2.

## Request/Response Contract

The Remote HTTPS provider speaks a simple JSON wire format compatible with WhisperX-style workers.

### Request

```json
{
  "audio_base64": "<base64-encoded WAV bytes>",
  "model": "whisperx-large-v2",
  "language": "en",
  "min_speakers": 1,
  "max_speakers": 5
}
```

- `audio_base64`: Required. 16kHz mono 16-bit PCM WAV.
- `model`: Required. Model identifier your backend recognizes.
- `language`: Optional. ISO language code.
- `min_speakers`, `max_speakers`: Optional. Passed to diarization models.

### Response

```json
{
  "segments": [
    {"start": 0.0, "end": 1.5, "text": "hello world", "speaker": "SPEAKER_00"},
    {"start": 1.5, "end": 3.0, "text": "how are you", "speaker": "SPEAKER_01"}
  ],
  "error": null
}
```

- `segments`: Array of transcribed segments. Each has `start`, `end`, `text`. Speaker diarization is indicated by the optional `speaker` field.
- `error`: If present with HTTP 200, treated as a worker error.

### Speaker Diarization Convention

Segments with a `speaker` field are rendered as `SPEAKER_XX: text`. Segments without `speaker` are rendered as plain lines. Empty text fields in segments are ignored.

## Privacy

The Remote HTTPS provider **uploads your entire meeting audio** to the configured endpoint. If privacy is a concern, use `localWhisper` or `parakeet` instead, or ensure your endpoint runs on infrastructure you control.

## Errors

| Condition | Result |
|-----------|--------|
| HTTP 4xx/5xx | `TranscriptionError::EngineFailed` with status code |
| `{"error": "..."}` (HTTP 200) | `TranscriptionError::EngineFailed` with error message |
| JSON parse failure | `TranscriptionError::EngineFailed` with parse error |
| Missing `endpoint_url` | `TranscriptionError::EngineFailed` immediately, no request sent |