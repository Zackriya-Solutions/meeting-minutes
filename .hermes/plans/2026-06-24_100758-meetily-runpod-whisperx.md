# Speaker Diarization + Arabic Whisper on RunPod for Meetily (Path B — DEPLOYMENT CONFIG ONLY)

> ⚠️ **`runpod-worker/` is our *deployment configuration*, not part of meetily. It lives in the fork for ops convenience but never gets PR'd to `Zackriya-Solutions/meeting-minutes`.**

> **Upstreamable code = the 3-PR plan at `.hermes/plans/2026-06-24_140000-meetily-upstream-pr.md`. This file is the ops-side companion only.**

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Add a `runpodWhisper` provider to Meetily's `TranscriptionProvider` trait, and ship a RunPod serverless worker image that runs Whisper large-v2 with Arabic/code-switching + pyannote diarization. ~$0.024/meeting on RTX 3090. No upstream fork required.

**Architecture:** Two new artifacts:
1. A standalone RunPod serverless worker image (`runpod-worker/`) — Python + `whisperx` + `pyannote/speaker-diarization-3.1` + `Mano200600/faster-whisper-large-v2-ar-codeswitching`. Returns speaker-labeled JSON segments.
2. A new Rust provider in meetily `frontend/src-tauri/src/audio/transcription/runpod_provider.rs` — implements the existing `TranscriptionProvider` trait. Operates only on the **post-hoc path** (import + retranscribe), not live streaming (chunked streaming is cost-disastrous on cold-start; defer until ROI proven).

**Tech Stack:**
- RunPod serverless (handler.py + Dockerfile) — RTX 3090, container disk 10 GB
- `whisperx` 3.x with `faster-whisper` backend, `pyannote/speaker-diarization-3.1`
- `reqwest` (verify in Cargo.toml during Task 1; add only if not present)
- `tokio` (already in meetily) for async
- Frontend UI: a new "RunPod Whisper" choice in the existing **Settings → Transcription Provider** dropdown

**Repo:** `C:\Users\USER\projects\meetily` (cloned fresh from `Zackriya-Solutions/meetily` @ commit `0281737`, Merge PR #502 / v0.4.0 release)

---

## Prerequisites — verify before starting

| Check | Where | Why |
|-------|-------|-----|
| `reqwest` already in `Cargo.toml` | `frontend/src-tauri/Cargo.toml` | Don't add duplicate deps |
| `serde_json` already in | same | We need it for transcript payloads |
| `tokio` features enabled | same | Need `time` for polling, `fs` for WAV |
| `transcription::provider.rs` exposes `TranscriptResult { text, confidence: Option<f32>, is_partial: bool }` | `frontend/src-tauri/src/audio/transcription/provider.rs` | Trait constraints to satisfy |
| `database/repositories/setting.rs` knows provider list (`localWhisper, deepgram, elevenLabs, groq, openai`) | already verified | We append `runpodWhisper` |
| `retranscription.rs::decode_audio_file` returns `Vec<f32>` | already verified | The audio samples feed `transcribe(audio, lang)` |

If a prerequisite fails, stop and surface it. Do not silently substitute a different library.

---

# Phase 0 — Build the standalone RunPod worker

**Goal:** a self-contained Docker image that takes a `.wav` over HTTPS, runs WhisperX with code-switched Arabic ASR + pyannote diarization, returns speaker-labeled segments JSON. This is Path A from the synthesis — it works **without** any meetily change and proves end-to-end before we touch Rust.

**Live under:** `C:\Users\USER\projects\meetily\runpod-worker\` (colocated with meetily so future maintainers find it).

### Task 0.1: Worker skeleton files

**Files (all Create):**
- `runpod-worker/Dockerfile`
- `runpod-worker/requirements.txt`
- `runpod-worker/.env.example`
- `runpod-worker/scripts/test_local.py`

**Skeletons (full code):**

```dockerfile
# runpod-worker/Dockerfile
FROM nvidia/cuda:12.4.1-cudnn-runtime-ubuntu22.04
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3.11 python3.11-venv python3-pip git ffmpeg curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
ENV PYTHONUNBUFFERED=1
ENV HF_HOME=/app/.cache/huggingface
ENV TRANSFORMERS_CACHE=/app/.cache/huggingface/transformers
COPY requirements.txt .
RUN python3.11 -m pip install --no-cache-dir -r requirements.txt
COPY handler.py .
CMD ["python3.11", "-u", "handler.py"]
```

```text
# runpod-worker/requirements.txt
runpod==1.7.*
whisperx==3.4.*
torch==2.4.*
pyannote-audio==3.1.*
faster-whisper==1.1.*
```

```bash
# runpod-worker/.env.example  (load with python-dotenv at startup)
HF_TOKEN=hf_xxxxxxxxxxxxxxxxxxxx
DEFAULT_MODEL=Mano200600/faster-whisper-large-v2-ar-codeswitching
DEFAULT_LANGUAGE=ar
USE_AUTH_TOKEN_HEADER=true
```

```python
# runpod-worker/scripts/test_local.py
"""Local smoke test -- run after building the image, but before pushing to RunPod."""
import base64, json, sys
from handler import handler

if len(sys.argv) < 2:
    print("usage: test_local.py <path-to-wav>")
    sys.exit(2)

wav_path = sys.argv[1]
with open(wav_path, "rb") as f:
    audio_b64 = base64.b64encode(f.read()).decode()

event = {"input": {"audio_base64": audio_b64, "language": "ar"}}
result = handler(event)
print(json.dumps(result, indent=2)[:4000])
```

### Task 0.2: Write `handler.py` (full content)

**File:** `runpod-worker/handler.py`

```python
"""RunPod serverless handler for WhisperX + pyannote diarization.

job_input fields:
  audio_base64 : str   -- base64-encoded WAV
  language     : str   -- ISO-639-1 (default "ar")
  model        : str   -- HF repo id
  min_speakers : int   -- optional
  max_speakers : int   -- optional

Returns:
  segments : [{start, end, text, speaker}]
  speakers : ["SPEAKER_00", "SPEAKER_01", ...]   (sorted)
  model    : <repo_id>
"""
import os, base64, tempfile, json, traceback
import runpod
import torch

_MODEL = _ALIGN = _META = _DIARIZE = None
_DEVICE = "cuda" if torch.cuda.is_available() else "cpu"

def _load(model_id: str) -> None:
    global _MODEL, _ALIGN, _META, _DIARIZE
    if _MODEL is not None:
        return
    import whisperx
    compute_type = "float16" if _DEVICE == "cuda" else "int8"
    hf_token = os.environ.get("HF_TOKEN")
    _MODEL = whisperx.load_model(model_id, device=_DEVICE, compute_type=compute_type)
    _ALIGN, _META = whisperx.load_align_model(language_code="ar", device=_DEVICE)
    _DIARIZE = whisperx.DiarizationPipeline(use_auth_token=hf_token, device=_DEVICE)

def handler(event):
    job_input = event.get("input", {})
    audio_b64 = job_input.get("audio_base64")
    if not audio_b64:
        return {"error": "audio_base64 is required"}
    language = job_input.get("language") or os.environ.get("DEFAULT_LANGUAGE", "ar")
    model_id = job_input.get("model") or os.environ.get(
        "DEFAULT_MODEL",
        "Mano200600/faster-whisper-large-v2-ar-codeswitching",
    )

    try:
        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tf:
            tf.write(base64.b64decode(audio_b64))
            wav_path = tf.name

        import whisperx
        _load(model_id)

        audio = whisperx.load_audio(wav_path)
        result = _MODEL.transcribe(audio, language=language)
        result = whisperx.align(result["segments"], _ALIGN, _META, audio, device=_DEVICE)

        diar_kwargs = {}
        if (ms := job_input.get("min_speakers")) is not None:
            diar_kwargs["min_speakers"] = ms
        if (mx := job_input.get("max_speakers")) is not None:
            diar_kwargs["max_speakers"] = mx
        diarize_segments = _DIARIZE(audio, **diar_kwargs)
        result = whisperx.assign_word_speakers(diarize_segments, result)

        out = []
        speakers = set()
        for seg in result.get("segments", []):
            spk = seg.get("speaker") or "UNKNOWN"
            speakers.add(spk)
            out.append({
                "start": float(seg["start"]),
                "end": float(seg["end"]),
                "text": seg["text"].strip(),
                "speaker": spk,
            })
        return {
            "segments": out,
            "speakers": sorted(speakers),
            "model": model_id,
        }
    except Exception as exc:
        return {"error": str(exc), "trace": traceback.format_exc()}
    finally:
        try:
            os.unlink(wav_path)
        except OSError:
            pass

runpod.serverless.start({"handler": handler})
```

### Task 0.3: Local smoke test on existing captured audio

**Goal:** prove handler.py processes a real WAV before we touch RunPod. We'll use one of our earlier `Meeting_*.wav` recordings or our `test_record_*.wav`.

```cmd
cd C:\Users\USER\projects\meetily\runpod-worker
docker build -t meetily-runpod-whisperx:latest .
docker run --rm --gpus all -e HF_TOKEN=hf_xxx ^
    -v "$PWD:/app" meetily-runpod-whisperx:latest ^
    python3.11 scripts/test_local.py /test_audio/test.wav
```

**Expected:** a JSON chunk with `segments[]` populated; at least one segment with `text` and `speaker`.

### Task 0.4: Push image + create serverless endpoint

```cmd
docker tag meetily-runpod-whisperx:latest <registry>/<image>:v0.1.0
docker push <registry>/<image>:v0.1.0
```

Then on RunPod:
1. Console → Serverless → New Endpoint
2. Container image: the image above
3. GPU: `RTX 3090`
4. Container disk: `10 GB` (one-time load batches of pyannote + faster-whisper ~6 GB)
5. Worker env vars: `HF_TOKEN`, `DEFAULT_MODEL`, `DEFAULT_LANGUAGE`
6. Max workers: `2` initially (each costs $0.36/hr while idle; bump to 5–10 once we know steady-state)
7. Idle timeout: `5 s` (kills workers between meetings → cold-start cost)
8. Save endpoint ID and `RUNPOD_API_KEY` to a local `.env` (not committed).

### Task 0.5: Smoke-test the live endpoint from curl

```cmd
curl -X POST https://api.runpod.ai/v2/<endpoint_id>/runsync ^
  -H "Authorization: Bearer <RUNPOD_API_KEY>" ^
  -H "Content-Type: application/json" ^
  -d "$(jq -n --rawfile wav /test_audio/test.wav '{input: {audio_base64: $wav|@base64}, language: "ar"}')"
```

**Expected (truncated):**
```json
{
  "segments": [
    {"start": 0.0, "end": 4.5, "text": "أهلا وسهلا يا فريق", "speaker": "SPEAKER_00"},
    ...
  ],
  "speakers": ["SPEAKER_00", "SPEAKER_01"],
  "model": "Mano200600/faster-whisper-large-v2-ar-codeswitching"
}
```

If `error` field is non-null, dump `trace` and fix the handler before moving to Phase 1.

---

# Phase 1 — Add `runpodWhisper` provider to meetily (post-hoc path only)

**Goal:** users can pick "RunPod Whisper" from the transcription provider dropdown, set API key + endpoint ID, and import/retranscribe any saved `.wav` → JSON segments with speakers rendered as `SPEAKER NN: text` lines.

### Task 1.1: Settings + DB schema (add provider)

**File:** `frontend/src-tauri/src/database/repositories/setting.rs`

**Edit:** extend `save_transcript_api_key`'s `match provider` block to handle `runpodWhisper`. The DB has columns `whisperApiKey, deepgramApiKey, elevenLabsApiKey, groqApiKey, openaiApiKey`. Reuse `whisperApiKey` for the RunPod key (it's the same field: "transcription provider API key"). No new migration needed.

```rust
// existing code at line ~180
let api_key_column = match provider {
    "localWhisper" => "whisperApiKey",   // <- reused for runpodWhisper
    "parakeet" => return Ok(()),
    "deepgram" => "deepgramApiKey",
    "runpodWhisper" => "whisperApiKey",  // ← ADD THIS LINE
    // ...
};
```

Also add `runpodWhisper` to the comment list on line ~27 (`localWhisper, deepgram, elevenLabs, groq, openai, runpodWhisper`).

### Task 1.2: Add `runpod_endpoint_id` field to settings JSON

Meetily stores **provider-specific** settings in a JSON blob, not just the API key. Verify by reading `setting.rs` file 70–120. If the storage is a flat `whisperApiKey VARCHAR`, then we need an extra column for the endpoint ID. If it's already JSON, add `runpodEndpointId` to the JSON shape.

**Files:**
- Modify: `frontend/src-tauri/src/database/migrations/*` — add new column `runpodEndpointId VARCHAR(255)` if missing
- Modify: `frontend/src-tauri/src/database/repositories/setting.rs` — get/set for the new key

(If migrations directory uses SQLx-style auto-managed schema, prefer to `ALTER TABLE`, otherwise add a new `*_runpod*.sql` migration.)

### Task 1.3: Implement `RunpodWhisperProvider`

**File (Create):** `frontend/src-tauri/src/audio/transcription/runpod_provider.rs`

```rust
//! RunPod WhisperX provider for meetily.
//!
//! Wire-protocol JSON between meetily and the RunPod endpoint:
//!   POST /v2/{endpoint_id}/runsync   (sync mode)
//!     Authorization: Bearer <api_key>
//!     Body: {"input": {"audio_base64": "..."|path?, "language": "ar", "model": "..."}}
//!
//! Response:
//!   {"segments": [{start, end, text, speaker}], "speakers": [...]}

use super::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use async_trait::async_trait;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone)]
pub struct RunpodWhisperProvider {
    pub endpoint_id:    String,   // RunPod endpoint UUID
    pub api_key:        String,   // RUNPOD_API_KEY
    pub model:          String,   // HF repo id, e.g. Mano200600/faster-whisper-large-v2-ar-codeswitching
    pub default_lang:   String,   // "ar"
    pub base_url:       String,   // "https://api.runpod.ai/v2"
    pub http:           reqwest::Client,
}

#[derive(Serialize)]
struct JobInput<'a> {
    audio_base64:  String,
    language:      &'a str,
    model:         &'a str,
    min_speakers:  Option<u8>,
    max_speakers:  Option<u8>,
}

#[derive(Deserialize)]
struct Segment {
    start:   f64,
    end:     f64,
    text:    String,
    speaker: String,
}

#[derive(Deserialize)]
struct RunpodOutput {
    segments: Option<Vec<Segment>>,
    speakers: Option<Vec<String>>,
    error:    Option<String>,
    #[serde(default)]
    _trace:   Option<String>,
}

impl RunpodWhisperProvider {
    pub fn new(endpoint_id: String, api_key: String, model: String, default_lang: String) -> Self {
        Self {
            endpoint_id,
            api_key,
            model,
            default_lang,
            base_url: "https://api.runpod.ai/v2".to_string(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(300))       // cold start + 60-min job cushion
                .connect_timeout(Duration::from_secs(15))
                .build()
                .expect("reqwest client build"),
        }
    }

    /// Build a transcript string with speaker labels — what `transcribe()` returns.
    /// Format: "SPEAKER_00: <text>\nSPEAKER_01: <text>"
    fn format_with_speakers(segments: &[Segment]) -> (String, f32) {
        if segments.is_empty() { return ("".to_string(), 1.0); }
        // Keep per-speaker groups continuous if segments alternate rapidly;
        // otherwise render turn-by-turn.
        let mut text = String::with_capacity(segments.len() * 120);
        let mut last_speaker: Option<&str> = None;
        for s in segments {
            if Some(s.speaker.as_str()) != last_speaker {
                text.push_str(&format!("{}: ", s.speaker));
                last_speaker = Some(s.speaker.as_str());
            }
            text.push_str(&s.text);
            text.push('\n');
        }
        (text, 1.0)  // confidence not exposed by whisperx; report 1.0 to satisfy the trait
    }
}

#[async_trait]
impl TranscriptionProvider for RunpodWhisperProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        // convert f32 PCM samples (16kHz mono, per the trait doc) → std::wav PCM_16
        let mut wav_buf = Vec::with_capacity(44 + audio.len() * 2);
        wav::write(
            wav::Header {
                audio_format: 1,                // PCM
                num_channels: 1,
                sample_rate: 16000,
                bits_per_sample: 16,
                num_samples: audio.len() as u32,
            },
            &wav::BitDepth::Sixteen(
                audio.iter().map(|f| (f.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).collect(),
            ),
            &mut std::io::Cursor::new(&mut wav_buf),
        ).map_err(|e| TranscriptionError::EngineFailed(format!("wav-encode: {e}")))?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&wav_buf);

        let job = JobInput {
            audio_base64: b64,
            language:     language.as_deref().unwrap_or(&self.default_lang),
            model:        &self.model,
            min_speakers: None,
            max_speakers: None,
        };
        let url = format!("{}/{}/runsync", self.base_url, self.endpoint_id);
        let resp = self.http.post(&url)
            .bearer_auth(&self.api_key)
            .json(&serde_json::json!({ "input": job }))
            .send().await
            .map_err(|e| TranscriptionError::EngineFailed(format!("runpod-send: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(TranscriptionError::EngineFailed(format!("runpod-http {}: {}", status, body)));
        }

        let out: RunpodOutput = resp.json()
            .await
            .map_err(|e| TranscriptionError::EngineFailed(format!("runpod-parse: {e}")))?;
        if let Some(err) = out.error {
            return Err(TranscriptionError::EngineFailed(format!("runpod-handler-error: {err}")));
        }
        let segments = out.segments.unwrap_or_default();
        let (text, conf) = Self::format_with_speakers(&segments);
        Ok(TranscriptResult { text, confidence: Some(conf), is_partial: false })
    }

    async fn is_model_loaded(&self) -> bool {
        // Serverless providers don't really "load" locally — server defines it.
        !self.endpoint_id.is_empty() && !self.api_key.is_empty()
    }

    async fn get_current_model(&self) -> Option<String> { Some(self.model.clone()) }

    fn provider_name(&self) -> &'static str { "runpodWhisper" }
}
```

**Crate dependencies to verify (Task 1.1 prerequisite):**
- `frontend/src-tauri/Cargo.toml` must have: `reqwest = { version = "0.12", features = ["json", "rustls-tls"] }`, `serde_json`, `base64`, `tokio`, `async-trait`. **If `wav` crate is not present, add `wav = "1.0"`** so we can encode f32→16-bit PCM.

### Task 1.4: Wire the provider into `TranscriptionEngine`

**File:** `frontend/src-tauri/src/audio/transcription/engine.rs`

**Edit two places:**

1. In `validate_transcription_model_ready`, extend the `match config.provider.as_str()` block:

```rust
"runpodWhisper" => {
    info!("🛰️ Validating RunPod endpoint...");
    let cfg = api_get_transcript_config(app.clone(), app.clone().state(), None).await?;
    // (cfg already in scope as `config` above; reuse it)
    if config.api_key.is_none() || config.api_key.as_deref() == Some("") {
        return Err("RunPod API key not set".into());
    }
    // The endpoint_id is stored separately; load from settings or env
    let endpoint_id = std::env::var("MEETILY_RUNPOD_ENDPOINT_ID").map_err(|e| e.to_string())?;
    Ok(())
}
```

2. In `get_or_init_transcription_engine`, extend the `match` with a new branch that returns `TranscriptionEngine::Provider(Arc::new(RunpodProvider::new(...)))`.

(Use `api_get_transcript_config` once, extract `(provider, model, api_key)`, plus `endpoint_id` from env or a new Tauri command — see Task 1.5.)

### Task 1.5: Tauri command `runpod_set_endpoint`

**File (Create):** `frontend/src-tauri/src/commands/runpod.rs`

```rust
#[tauri::command]
pub async fn set_runpod_endpoint(
    state: tauri::State<'_, crate::state::AppState>,
    endpoint_id: String,
    api_key: String,
    model: Option<String>,
    language: Option<String>,
) -> Result<(), String> {
    use crate::database::repositories::setting::SettingsRepository;
    SettingsRepository::save_setting_value(
        state.db_manager.pool(), "runpodEndpointId", &endpoint_id
    ).await.map_err(|e| e.to_string())?;
    if let Some(m) = model {
        SettingsRepository::save_setting_value(
            state.db_manager.pool(), "runpodModel", &m
        ).await.map_err(|e| e.to_string())?;
    }
    if let Some(l) = language {
        SettingsRepository::save_setting_value(
            state.db_manager.pool(), "runpodLanguage", &l
        ).await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_runpod_config(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<serde_json::Value, String> {
    use crate::database::repositories::setting::SettingsRepository;
    let endpoint = SettingsRepository::get_setting_value(
        state.db_manager.pool(), "runpodEndpointId"
    ).await.map_err(|e| e.to_string())?;
    let model = SettingsRepository::get_setting_value(
        state.db_manager.pool(), "runpodModel"
    ).await.map_err(|e| e.to_string())?;
    let language = SettingsRepository::get_setting_value(
        state.db_manager.pool(), "runpodLanguage"
    ).await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "endpointId": endpoint,
        "model": model.unwrap_or_else(|| "Mano200600/faster-whisper-large-v2-ar-codeswitching".into()),
        "language": language.unwrap_or_else(|| "ar".into()),
    }))
}
```

`SettingsRepository::save_setting_value` and `get_setting_value` may need to exist — if they don't (some repos only have domain-specific helpers), add them using the tooling already in `setting.rs`. Otherwise store via a new typed `SaveSettingRequest { key, value }` API endpoint that just calls `UPDATE settings SET value=? WHERE key=?`.

Also register both commands in `frontend/src-tauri/src/lib.rs::invoke_handler!`.

### Task 1.6: Hook into retranscription path

**File:** `frontend/src-tauri/src/audio/retranscription.rs`

Locate the function that calls `engine.transcribe(...)` and add a branch:

```rust
match engine.provider_name() {
    "runpodWhisper" => {
        // The audio is already decoded as Vec<f32>; call transcribe_vec.
        // Runpod returns speaker-labeled text. Persist `segments` JSON
        // alongside transcripts so the UI can render colored speaker rows.
        let resp = engine.transcribe(chunks_f32, Some(lang.to_string())).await?;
        // existing save logic; just add a `speakers` field if the provider
        // surfaced them — see Task 1.7.
    }
    _ => { /* existing branch */ }
}
```

(Touching live code paths; tests are essential here — see Verification.)

### Task 1.7: Persist speaker metadata on segment rows

Transcripts currently store `(text, timestamp, audio_start_time, audio_end_time, duration)` per row (see `MeetingTranscript` struct in `api.rs`). Add an optional `speaker: Option<String>` column to that struct + the database table (one migration). The RunPod path populates it on retranscribe; existing local paths leave it `NULL`.

### Task 1.8: Frontend UI — provider picker + RunPod fields

**File:** `frontend/src/components/settings/transcription/*.tsx`

There will already be a settings page for "transcription provider." Add to its provider list:

```tsx
<option value="runpodWhisper">RunPod Whisper (cloud, ~$0.024/meeting)</option>
```

When `runpodWhisper` is selected, show three extra fields:
- Endpoint ID (text)
- API Key (password)
- Model (text, default `Mano200600/faster-whisper-large-v2-ar-codeswitching`)
- Language (select, default `ar`)

Wire `set_runpod_endpoint` / `get_runpod_config`.

This is a small (~30 LoC) UI change. There's no test infrastructure for React given the scaffold, so do **visual smoke-test only** (build with `pnpm run tauri:dev`).

---

# Phase 2 — Validation, integration test, commit

### Task 2.1: End-to-end test on a real recorded meeting

1. Run the updated `record_meeting.bat` for at least 5 minutes of a fake / real meeting.
2. In meetily, set provider = `runpodWhisper`, paste endpoint ID + API key.
3. Trigger retranscribe on the resulting `.wav`.
4. Verify the transcript comes back with `SPEAKER_00:` / `SPEAKER_01:` text labels.
5. Verify cost on RunPod: should be ~$0.024 for a 5-min meeting.

### Task 2.2: Trait-unit tests

**File:** `frontend/src-tauri/src/audio/transcription/runpod_provider.rs` (append)

Light tests using `mockito` or `wiremock` to fake a returns-from-runpod JSON. Verify:

- 200 OK with 2 segments → text starts with `SPEAKER_00:`
- 200 OK with empty segments → empty text, confidence 1.0
- 4xx → `EngineFailed`
- 200 OK with `error` field set → `EngineFailed`

### Task 2.3: Final cargo build + tauri:dev sanity

```cmd
cd C:\Users\USER\projects\meetily\frontend
pnpm install
pnpm run tauri:dev:cuda   (or cpu)
```

Verify:
- [ ] App builds without warnings (other than existing)
- [ ] Settings UI loads, picker shows "RunPod Whisper"
- [ ] Clicking the picker exposes the endpoint fields
- [ ] Saving settings persists across app restart
- [ ] Recording + retranscribing emits speaker-labeled text

---

# Files likely to change

| Path                                                                                   | Change        |
|----------------------------------------------------------------------------------------|---------------|
| `runpod-worker/Dockerfile`                                                             | NEW           |
| `runpod-worker/handler.py`                                                             | NEW           |
| `runpod-worker/requirements.txt`                                                       | NEW           |
| `runpod-worker/.env.example`                                                           | NEW           |
| `runpod-worker/scripts/test_local.py`                                                  | NEW           |
| `frontend/src-tauri/src/audio/transcription/runpod_provider.rs`                        | NEW (~150 LoC) |
| `frontend/src-tauri/src/commands/runpod.rs`                                            | NEW (~70 LoC)  |
| `frontend/src-tauri/Cargo.toml`                                                        | MAYBE NEW DEPS|
| `frontend/src-tauri/src/audio/transcription/mod.rs`                                    | MODIFY (export)|
| `frontend/src-tauri/src/audio/transcription/engine.rs`                                 | MODIFY (2 functions) |
| `frontend/src-tauri/src/audio/retranscription.rs`                                      | MODIFY (1 match) |
| `frontend/src-tauri/src/database/repositories/setting.rs`                              | MODIFY (provider list) |
| `frontend/src-tauri/src/database/migrations/<next>.sql`                                | NEW (speaker col + runpod fields) |
| `frontend/src-tauri/src/api/api.rs::MeetingTranscript`                                 | MODIFY (speaker field) |
| `frontend/src-tauri/src/lib.rs::invoke_handler`                                        | MODIFY (register 3 commands) |
| `frontend/src/components/settings/transcription/*.tsx`                                 | MODIFY (UI)   |
| `frontend/src-tauri/src/audio/transcription/runpod_provider.rs::tests`                  | NEW (~40 LoC) |

Estimated total LoC: **~750** (worker: 200, Rust: 350, TS: 100, tests: 100, docs: 50).

---

# Risks, tradeoffs, open questions

1. **Cold-start latency on the first retranscription**: 5–15 s before the worker is warm. Meetily's UI must show a spinner. Acceptable for the post-hoc path (already an offline operation).
2. **Audio base64 size**: a 60-min meeting WAV at 16 kHz mono is ~115 MB. We test the small-payload path (<10 MB) first; for big meetings, the user has to upload via the RunPod Network Volume (out of scope for V1 — flag as deferred).
3. **`wav` crate as a new dep**: adds ~25 transitive crates. Acceptable; if the user objects, we can hand-roll a 44-byte WAV header + `i16` writes directly into a `Vec<u8>`. LoC ~30.
4. **Provider list in the DB**: extending it requires recreating the picker on every UI load. The change should be additive — never break existing `localWhisper` users.
5. **Language detection**: When users set `"language": "en"` we'd need a non-Arabic WhisperX align path. V1 ships with `"ar"` align only; this is a Phase 2 enhancement.
6. **pyannote model license**: requires `HF_TOKEN` accepting user conditions for the segmentation + diarization repos. Documented in the README; one-time IT setup.
7. **Module placement**: where the `commands` mod sits in `lib.rs` matters for hot-reload during `tauri:dev`. Match the entry style of existing commands (e.g. `command.rs` for `set_runpod_endpoint` is more idiomatic than `runpod.rs` — verify before writing).

---

# Out of scope (carry over notes)

- Live streaming path for RunPod. Cost model makes this impractical until RunPod adds warm-pool pricing for tiny per-chunk jobs. The trait allows it (just call `transcribe()` on each chunk), so the wiring is feasible later — but today, post-hoc only.
- Multi-speaker diarization export to SRT/VTT. The JSON shape is rich enough to generate both client-side.
- Local fallback (faster-whisper int8 on user's CPU). Use same provider trait, swap runner. Future work.

---

# Verification checklist (gate to mark complete)

- [ ] Worker Phase 0: RunPod endpoint exists and returns valid JSON for a 1-min WAV.
- [ ] Cargo check (`cd frontend/src-tauri && cargo check`) → 0 new warnings.
- [ ] Frontend builds (`pnpm install && pnpm run tauri:dev:cpu`).
- [ ] UI picker shows "RunPod Whisper" with the three required fields.
- [ ] End-to-end: import a WAV via the existing import flow with provider=runpodWhisper → segments show speaker prefixes.
- [ ] Trait unit tests pass: `cargo test -p <crate> transcription::runpod_provider`.

---

# Plan summary

20 tasks across 2 phases. Workers (Phase 0) stand up first so we have a live endpoint to point at. Phase 1 adds the meetily provider and validates end-to-end against the live endpoint. Most risks are size-related (cold-start, audio payload size), and the biggest win is decoupling the **capture** (already built) from the **off-box compute** (this plan).
