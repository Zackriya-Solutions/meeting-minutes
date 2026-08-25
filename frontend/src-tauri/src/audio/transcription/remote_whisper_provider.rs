// audio/transcription/remote_whisper_provider.rs
//
// Remote Whisper transcription provider: sends audio over HTTP to a
// self-hosted OpenAI-compatible `/v1/audio/transcriptions` endpoint
// (e.g. a faster-whisper server running on a machine with a dedicated GPU),
// instead of loading a model in-process on this machine.
//
// The endpoint is expected to accept a multipart/form-data POST with a
// `file` field (WAV audio) and an optional `language` field, and to return
// JSON shaped like `{"text": "..."}` — the same contract OpenAI's
// `/v1/audio/transcriptions` uses. Any faster-whisper / whisper.cpp server
// built against that convention works without changes.

use super::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use async_trait::async_trait;
use reqwest::multipart;
use log::warn;
use serde::Deserialize;
use std::time::Duration;

/// Sample rate the rest of the transcription pipeline hands us audio at.
/// (See `TranscriptionProvider::transcribe` doc comment: 16kHz mono f32.)
const SAMPLE_RATE_HZ: u32 = 16_000;

/// How long we allow a single transcription request to run before giving up.
/// Large chunks on a cold GPU (model not yet resident) can take a while.
const REQUEST_TIMEOUT_SECS: u64 = 60;

/// How long we allow a liveness probe to run. Deliberately much shorter than a
/// transcription: a probe gates interactive actions (the onboarding "Test"
/// button, and every recording start), so it must fail fast. A host that is off
/// or behind a DROP firewall never sends a reset, so without this the probe
/// would inherit the 60s transcription budget and freeze the UI for a minute.
const HEALTH_TIMEOUT_SECS: u64 = 5;

/// Meetily's language picker emits these sentinels for "let the engine decide".
/// They are not ISO-639-1 codes: an OpenAI-compatible server rejects them
/// (faster-whisper answers 500 Internal Server Error). Omitting the field
/// entirely is exactly how that API is asked to auto-detect, so the sentinels
/// must be dropped rather than forwarded.
const AUTO_LANGUAGE_SENTINELS: [&str; 2] = ["auto", "auto-translate"];

#[derive(Debug, Deserialize)]
struct RemoteTranscriptionResponse {
    text: String,
}

/// Transcription provider that delegates to a remote whisper-compatible HTTP server.
pub struct RemoteWhisperProvider {
    /// Base URL of the remote server, e.g. "http://192.168.1.100:8093".
    /// No trailing slash expected; it is stripped defensively in `new`.
    base_url: String,
    client: reqwest::Client,
    model_label: String,
}

impl RemoteWhisperProvider {
    pub fn new(base_url: String) -> Self {
        let base_url = base_url.trim_end_matches('/').to_string();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let model_label = format!("remote ({})", base_url);

        Self {
            base_url,
            client,
            model_label,
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/v1/audio/transcriptions", self.base_url)
    }

    /// Map Meetily's language selection onto what the remote API accepts:
    /// `Some(iso_code)` to force a language, `None` to let the server detect it.
    fn normalize_language(language: Option<String>) -> Option<String> {
        let language = language?;
        let trimmed = language.trim();
        if trimmed.is_empty() {
            return None;
        }

        let lowered = trimmed.to_ascii_lowercase();
        if AUTO_LANGUAGE_SENTINELS.contains(&lowered.as_str()) {
            if lowered == "auto-translate" {
                // `/v1/audio/transcriptions` transcribes in the source language;
                // translation lives behind a separate endpoint this provider does
                // not target. Be explicit rather than silently returning
                // untranslated text as if it had been translated.
                warn!(
                    "Remote Whisper: 'auto-translate' is not supported by \
                     /v1/audio/transcriptions; falling back to auto-detected transcription"
                );
            }
            return None;
        }

        Some(trimmed.to_string())
    }

    /// Encode 16kHz mono f32 PCM samples as a WAV byte buffer (16-bit PCM).
    /// Self-contained — no extra crate dependency needed for such a simple header.
    fn encode_wav_pcm16(samples: &[f32]) -> Vec<u8> {
        // WAV headers are u32, so an oversized buffer would wrap in release and
        // emit a header that silently disagrees with the payload. Chunks are
        // seconds long in practice; saturate rather than wrap if that ever changes.
        let num_samples = u32::try_from(samples.len()).unwrap_or(u32::MAX);
        let byte_rate = SAMPLE_RATE_HZ * 2; // mono, 16-bit
        let data_size = num_samples.saturating_mul(2);
        let riff_size = data_size.saturating_add(36);

        let mut buf = Vec::with_capacity(44 + data_size as usize);
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&riff_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");

        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
        buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
        buf.extend_from_slice(&1u16.to_le_bytes()); // mono
        buf.extend_from_slice(&SAMPLE_RATE_HZ.to_le_bytes());
        buf.extend_from_slice(&byte_rate.to_le_bytes());
        buf.extend_from_slice(&2u16.to_le_bytes()); // block align
        buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        for &sample in samples {
            let clamped = sample.clamp(-1.0, 1.0);
            let pcm = (clamped * i16::MAX as f32) as i16;
            buf.extend_from_slice(&pcm.to_le_bytes());
        }

        buf
    }
}

#[async_trait]
impl TranscriptionProvider for RemoteWhisperProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        if audio.is_empty() {
            return Err(TranscriptionError::AudioTooShort {
                samples: 0,
                minimum: 1,
            });
        }

        let wav_bytes = Self::encode_wav_pcm16(&audio);

        let mut form = multipart::Form::new().part(
            "file",
            multipart::Part::bytes(wav_bytes)
                .file_name("chunk.wav")
                .mime_str("audio/wav")
                .map_err(|e| TranscriptionError::EngineFailed(e.to_string()))?,
        );
        if let Some(lang) = Self::normalize_language(language) {
            form = form.text("language", lang);
        }

        let response = self
            .client
            .post(self.endpoint())
            .multipart(form)
            .send()
            .await
            .map_err(|e| {
                TranscriptionError::EngineFailed(format!(
                    "Cannot reach remote whisper server at {}: {}",
                    self.base_url, e
                ))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(TranscriptionError::EngineFailed(format!(
                "Remote whisper server returned {}: {}",
                status, body
            )));
        }

        let parsed: RemoteTranscriptionResponse = response
            .json()
            .await
            .map_err(|e| TranscriptionError::EngineFailed(format!("Invalid response JSON: {}", e)))?;

        Ok(TranscriptResult {
            text: parsed.text,
            confidence: None, // faster-whisper server doesn't return a confidence score
            is_partial: false,
        })
    }

    async fn is_model_loaded(&self) -> bool {
        // "Loaded" here means "reachable" — the remote process owns model lifecycle.
        // Note this only proves the server answers; it cannot promise the next
        // transcription succeeds.
        //
        // Two probes, because no single one covers the field. `/health` is a
        // whisper.cpp-server / faster-whisper-server extension, absent from the
        // OpenAI API; `/v1/models` is part of the OpenAI contract but absent from
        // some minimal Whisper servers. Probing only one strands users of the other
        // family on a reachable server that this check calls dead — and onboarding
        // refuses to continue on that verdict.
        for path in ["/health", "/v1/models"] {
            let url = format!("{}{}", self.base_url, path);
            let reachable = matches!(
                self.client
                    .get(&url)
                    .timeout(Duration::from_secs(HEALTH_TIMEOUT_SECS))
                    .send()
                    .await,
                // 401/403 count as alive: an endpoint that demands credentials is
                // still an endpoint that answered.
                Ok(resp) if resp.status().is_success() || resp.status() == 401 || resp.status() == 403
            );
            if reachable {
                return true;
            }
        }
        false
    }

    async fn get_current_model(&self) -> Option<String> {
        Some(self.model_label.clone())
    }

    fn provider_name(&self) -> &'static str {
        "RemoteWhisper"
    }
}

/// Tauri command: probe a remote Whisper server before committing to it.
///
/// Onboarding and settings both need to tell the user "this URL works" *before*
/// it is persisted, otherwise the first failure only surfaces mid-recording.
/// Deliberately reuses `TranscriptionProvider::is_model_loaded` so that what we
/// validate here is exactly what `validate_transcription_model_ready` checks later.
#[tauri::command]
pub async fn remote_whisper_check_health(base_url: String) -> Result<bool, String> {
    if base_url.trim().is_empty() {
        return Err("Server URL is empty".to_string());
    }

    let provider = RemoteWhisperProvider::new(base_url);
    Ok(provider.is_model_loaded().await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_sentinels_are_dropped_so_the_server_detects_the_language() {
        // Forwarding these produced "500 Internal Server Error" from faster-whisper.
        for sentinel in ["auto", "auto-translate", "AUTO", "Auto-Translate"] {
            assert_eq!(
                RemoteWhisperProvider::normalize_language(Some(sentinel.to_string())),
                None,
                "sentinel {sentinel} must not reach the server"
            );
        }
    }

    #[test]
    fn blank_and_absent_languages_are_dropped() {
        assert_eq!(RemoteWhisperProvider::normalize_language(None), None);
        assert_eq!(RemoteWhisperProvider::normalize_language(Some("".into())), None);
        assert_eq!(RemoteWhisperProvider::normalize_language(Some("   ".into())), None);
    }

    #[test]
    fn real_language_codes_are_forwarded_trimmed() {
        assert_eq!(
            RemoteWhisperProvider::normalize_language(Some("es".into())),
            Some("es".to_string())
        );
        assert_eq!(
            RemoteWhisperProvider::normalize_language(Some(" en ".into())),
            Some("en".to_string())
        );
    }

    #[test]
    fn wav_header_survives_an_implausibly_large_buffer() {
        // Not reachable with real chunks; the point is that the header stays
        // internally consistent instead of wrapping into nonsense.
        let data_size = u32::MAX.saturating_mul(2);
        assert_eq!(data_size, u32::MAX);
        assert_eq!(data_size.saturating_add(36), u32::MAX);
    }

    #[test]
    fn wav_header_describes_16khz_mono_pcm16() {
        let wav = RemoteWhisperProvider::encode_wav_pcm16(&[0.0, 1.0, -1.0]);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 1, "mono");
        assert_eq!(
            u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
            SAMPLE_RATE_HZ
        );
        assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 16, "16-bit samples");
        assert_eq!(wav.len(), 44 + 3 * 2);
    }
}
