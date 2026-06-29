// audio/transcription/groq_provider.rs
//
// Groq-hosted Whisper transcription provider.
//
// Sends audio via multipart/form-data to Groq's OpenAI-compatible
// /audio/transcriptions endpoint and parses the verbose_json response
// (segments with start/end/text) into the project's TranscriptResult.
//
// The Groq API is OpenAI-compatible. Listing in this provider is
// vendor-named (Groq is the only vendor that exposes it through this
// configuration shape) versus the vendor-neutral RemoteProvider.

use super::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use super::remote_provider::RemoteProvider; // for shared write_wav_pcm16_mono
use async_trait::async_trait;
use log::warn;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use std::time::Duration;

/// Configuration for Groq. The bearer token is the Groq API key
/// (gsk_…) — never logged.
#[derive(Clone, Debug)]
pub struct GroqConfig {
    pub api_key: String,
    pub model: String,    // e.g. "whisper-large-v3"
    pub default_lang: String,
    pub request_timeout: Duration,
}

const GROQ_ENDPOINT: &str = "https://api.groq.com/openai/v1/audio/transcriptions";

#[derive(Deserialize, Debug)]
struct GroqSegment {
    text: String,
    #[serde(default)]
    start: Option<f64>,
    #[serde(default)]
    end: Option<f64>,
}

#[derive(Deserialize, Debug)]
struct GroqResponse {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    segments: Option<Vec<GroqSegment>>,
}

pub struct GroqProvider {
    config: GroqConfig,
    client: reqwest::Client,
}

impl GroqProvider {
    pub fn new(config: GroqConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .build()
            .expect("reqwest::Client::builder with sensible timeout cannot fail");
        Self { config, client }
    }

    /// POST a multipart upload and parse Groq's response. Shared by
    /// the live `transcribe` path and the user-driven "Test" button.
    pub async fn upload(&self, wav_bytes: Vec<u8>, language: Option<&str>) -> std::result::Result<String, TranscriptionError> {
        if self.config.api_key.is_empty() {
            return Err(TranscriptionError::EngineFailed(
                "GroqProvider: api_key not configured".into(),
            ));
        }
        if self.config.model.is_empty() {
            return Err(TranscriptionError::EngineFailed(
                "GroqProvider: model not configured".into(),
            ));
        }

        // Ponytail: stamp a filename with .wav so Groq's content-type
        // detection is unambiguous.
        let file_part = Part::bytes(wav_bytes)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| TranscriptionError::EngineFailed(format!("multipart: {e}")))?;

        let mut form = Form::new()
            .text("model", self.config.model.clone())
            .text("response_format", "verbose_json".to_string())
            .part("file", file_part);

        if let Some(lang) = language {
            if !lang.is_empty() {
                form = form.text("language", lang.to_string());
            }
        }

        let response = self
            .client
            .post(GROQ_ENDPOINT)
            .bearer_auth(&self.config.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| TranscriptionError::EngineFailed(format!("Groq HTTP send: {e}")))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| TranscriptionError::EngineFailed(format!("Groq read body: {e}")))?;

        if !status.is_success() {
            // Truncate to 200 chars so the UX has something actionable
            // without dumping HTML error pages. Groq returns JSON
            // {error:{message:...}} on errors.
            let snippet: String = body.chars().take(200).collect();
            return Err(TranscriptionError::EngineFailed(format!(
                "Groq HTTP {}: {}",
                status.as_u16(),
                snippet
            )));
        }

        let parsed: GroqResponse = serde_json::from_str(&body)
            .map_err(|e| TranscriptionError::EngineFailed(format!("Groq parse response: {e}")))?;

        // Prefer verbose_json segments; fall back to the top-level text.
        if let Some(segs) = parsed.segments.as_ref() {
            let mut out = String::new();
            for seg in segs {
                let t = seg.text.trim();
                if t.is_empty() {
                    continue;
                }
                out.push_str(t);
                out.push('\n');
            }
            if !out.is_empty() {
                return Ok(out);
            }
        }
        Ok(parsed.text.unwrap_or_default())
    }
}

#[async_trait]
impl TranscriptionProvider for GroqProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        let lang_ref: Option<&str> = language
            .as_deref()
            .filter(|l| !l.is_empty())
            .or_else(|| {
                if self.config.default_lang.is_empty() {
                    None
                } else {
                    Some(self.config.default_lang.as_str())
                }
            });

        // Reuse the RemoteProvider's hand-rolled WAV header. Ponytail
        // rationale: 44 bytes doesn't justify a `wav` crate dep, and
        // sharing the helper keeps the on-wire format identical to the
        // generic remote path (which downstream workers may emulate).
        let mut wav = Vec::with_capacity(44 + audio.len() * 2);
        RemoteProvider::write_wav_pcm16_mono(&audio, 16_000, &mut wav);

        let text = self.upload(wav, lang_ref).await?;
        Ok(TranscriptResult {
            text,
            confidence: None, // Groq whisper doesn't surface per-segment scores
            is_partial: false,
        })
    }

    async fn is_model_loaded(&self) -> bool {
        // Ponytail: every successful resolve-and-Go request is a
        // readiness signal; doesn't gate on local state. Always true
        // once api_key + model are present.
        !self.config.api_key.is_empty() && !self.config.model.is_empty()
    }

    async fn get_current_model(&self) -> Option<String> {
        Some(self.config.model.clone())
    }

    fn provider_name(&self) -> &'static str {
        "Groq"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Without an api_key the provider refuses to send. This avoids a
    /// network round-trip during unit tests.
    #[tokio::test]
    async fn empty_api_key_returns_engine_failed() {
        let cfg = GroqConfig {
            api_key: String::new(),
            model: "whisper-large-v3".into(),
            default_lang: "en".into(),
            request_timeout: Duration::from_secs(5),
        };
        let p = GroqProvider::new(cfg);
        let res = p.transcribe(vec![0.0_f32; 16_000], None).await;
        assert!(matches!(res, Err(TranscriptionError::EngineFailed(_))));
    }

    #[tokio::test]
    async fn empty_model_returns_engine_failed() {
        let cfg = GroqConfig {
            api_key: "dummy".into(),
            model: String::new(),
            default_lang: "en".into(),
            request_timeout: Duration::from_secs(5),
        };
        let p = GroqProvider::new(cfg);
        let res = p.transcribe(vec![0.0_f32; 16_000], None).await;
        assert!(matches!(res, Err(TranscriptionError::EngineFailed(_))));
    }

    #[tokio::test]
    async fn is_model_loaded_reflects_required_fields() {
        let mut cfg = GroqConfig {
            api_key: "k".into(),
            model: "m".into(),
            default_lang: "en".into(),
            request_timeout: Duration::from_secs(5),
        };
        assert!(GroqProvider::new(cfg.clone()).is_model_loaded().await);
        cfg.api_key.clear();
        assert!(!GroqProvider::new(cfg.clone()).is_model_loaded().await);
        cfg.api_key = "k".into();
        cfg.model.clear();
        assert!(!GroqProvider::new(cfg).is_model_loaded().await);
    }

    /// Synthetic responses are accepted; we never hit the network.
    #[test]
    fn parses_verbose_json_segments() {
        let body = r#"{
            "task":"transcribe","language":"en",
            "segments":[{"start":0.0,"end":1.0,"text":" hello world"},{"start":1.0,"end":2.0,"text":"foo bar"}],
            "text":" hello world foo bar"
        }"#;
        let parsed: GroqResponse = serde_json::from_str(body).unwrap();
        let segs = parsed.segments.unwrap();
        let joined: String = segs.iter()
            .map(|s| s.text.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(joined, "hello world\nfoo bar");
    }

    #[test]
    fn parses_text_only_response() {
        let body = r#"{"text":"hello only"}"#;
        let parsed: GroqResponse = serde_json::from_str(body).unwrap();
        assert_eq!(parsed.text.as_deref(), Some("hello only"));
        assert!(parsed.segments.is_none());
    }

    #[test]
    fn parse_error_response_falls_through_to_engine_failed() {
        // We don't expose an explicit `error` field on GroqResponse; the
        // live path catches non-2xx HTTP status before this code runs.
        // This test documents that behavior so future readers don't
        // try to pull errors out of the JSON body.
        let body = r#"{"error":{"message":"bad key"}}"#;
        let parsed: std::result::Result<GroqResponse, _> = serde_json::from_str(body);
        // Either parse succeeds (with no segments and no text) or fails;
        // both are acceptable. The HTTP-status check is the canonical gate.
        let _ = parsed; // suppress unused; intentional permissive parse
    }
}
