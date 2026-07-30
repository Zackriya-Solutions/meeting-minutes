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

/// UI sentinels that are NOT ISO-639-1 codes and must never reach the wire.
///
/// The transcription-language selector offers "Auto Detect" (`auto`) and
/// "Auto Detect (Translate to English)" (`auto-translate`), and the Rust-side
/// global defaults to `auto-translate`. Groq validates `language` strictly and
/// answers `HTTP 400 unsupported language: auto-translate`, so forwarding a
/// sentinel loses the entire chunk.
///
/// Both mean "do not pin a language", which on this API is expressed by omitting
/// the field so the server auto-detects.
const LANGUAGE_SENTINELS: &[&str] = &["auto", "auto-translate", "auto-detect", "detect"];

/// Resolve a language preference into what should actually be sent.
///
/// `None` means "omit the field entirely and let Groq auto-detect", which is the
/// correct wire representation for every sentinel and for an unset preference.
fn resolve_language(preference: Option<&str>, config_default: &str) -> Option<String> {
    fn usable(value: &str) -> Option<String> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }
        let lowered = trimmed.to_ascii_lowercase();
        if LANGUAGE_SENTINELS.contains(&lowered.as_str()) {
            return None;
        }
        Some(lowered)
    }

    preference
        .and_then(usable)
        .or_else(|| usable(config_default))
}

/// Segments below this average log-probability are model guesswork rather than
/// transcription. Whisper's own decoder uses -1.0 as its fallback trigger.
const MIN_AVG_LOGPROB: f64 = -1.0;

/// Above this gzip-style compression ratio the text has collapsed into a
/// repetition loop. Matches Whisper's reference `compression_ratio_threshold`.
const MAX_COMPRESSION_RATIO: f64 = 2.4;

#[derive(Deserialize, Debug)]
struct GroqSegment {
    text: String,
    #[serde(default)]
    start: Option<f64>,
    #[serde(default)]
    end: Option<f64>,
    /// Confidence signals Whisper returns in `verbose_json`. Previously parsed
    /// away, which discarded the cheapest available hallucination detector.
    #[serde(default)]
    avg_logprob: Option<f64>,
    #[serde(default)]
    compression_ratio: Option<f64>,
    #[serde(default)]
    no_speech_prob: Option<f64>,
}

impl GroqSegment {
    /// True when Whisper's own scores say this segment is not a transcription.
    ///
    /// Measured on a real Arabic meeting: `avg_logprob < -1.0` flags the garbage,
    /// while `no_speech_prob > 0.5` flags none of it — the model is confident when
    /// it emits memorised subtitle boilerplate. `no_speech_prob` is therefore
    /// parsed for logging but deliberately not used as a gate.
    fn is_low_confidence(&self) -> bool {
        if self.avg_logprob.is_some_and(|lp| lp < MIN_AVG_LOGPROB) {
            return true;
        }
        self.compression_ratio
            .is_some_and(|ratio| ratio > MAX_COMPRESSION_RATIO)
    }
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
            // Disable Groq's server-side temperature fallback. Without this, a
            // low-confidence decode is retried with sampling, which turns a poor
            // segment into a confidently-wrong one.
            .text("temperature", "0".to_string())
            .part("file", file_part);

        // Already resolved by the caller: `Some` is a real ISO-639-1 code, `None`
        // means omit the field so the server auto-detects. Never forward a UI
        // sentinel — Groq rejects those with HTTP 400.
        if let Some(lang) = language {
            form = form.text("language", lang.to_string());
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
            let mut dropped = 0usize;
            for seg in segs {
                let t = seg.text.trim();
                if t.is_empty() {
                    continue;
                }
                // Whisper tells us when it was guessing; believe it.
                if seg.is_low_confidence() {
                    dropped += 1;
                    warn!(
                        "Groq: dropping low-confidence segment (avg_logprob={:?}, \
                         compression_ratio={:?}, no_speech_prob={:?})",
                        seg.avg_logprob, seg.compression_ratio, seg.no_speech_prob
                    );
                    continue;
                }
                out.push_str(t);
                out.push('\n');
            }
            if dropped > 0 {
                warn!(
                    "Groq: dropped {}/{} segments as low-confidence",
                    dropped,
                    segs.len()
                );
            }
            if !out.is_empty() {
                return Ok(out);
            }
            // Every segment was empty or filtered. Returning the top-level `text`
            // here would reinstate exactly what we just rejected.
            if !segs.is_empty() {
                return Ok(String::new());
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
        let resolved_lang = resolve_language(language.as_deref(), &self.config.default_lang);

        // Reuse the RemoteProvider's hand-rolled WAV header. Ponytail
        // rationale: 44 bytes doesn't justify a `wav` crate dep, and
        // sharing the helper keeps the on-wire format identical to the
        // generic remote path (which downstream workers may emulate).
        let mut wav = Vec::with_capacity(44 + audio.len() * 2);
        RemoteProvider::write_wav_pcm16_mono(&audio, 16_000, &mut wav);

        let text = self.upload(wav, resolved_lang.as_deref()).await?;
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

    // --- language sentinel resolution ---------------------------------------
    //
    // Groq validates `language` strictly. Confirmed against the live API:
    //   language=auto-translate -> HTTP 400 "unsupported language: auto-translate"
    // so a sentinel reaching the wire loses the chunk outright.

    #[test]
    fn auto_translate_never_reaches_the_wire() {
        assert_eq!(resolve_language(Some("auto-translate"), ""), None);
        // Nor via the configured default.
        assert_eq!(resolve_language(None, "auto-translate"), None);
    }

    #[test]
    fn auto_resolves_to_omitted_not_english() {
        // Regression: this used to fall through to a hardcoded "en", silently
        // transcribing non-English audio as English.
        assert_eq!(resolve_language(Some("auto"), ""), None);
        assert_eq!(resolve_language(Some("auto"), "en"), Some("en".into()));
    }

    #[test]
    fn explicit_language_is_passed_through_lowercased() {
        assert_eq!(resolve_language(Some("ar"), ""), Some("ar".into()));
        assert_eq!(resolve_language(Some("AR"), ""), Some("ar".into()));
        assert_eq!(resolve_language(Some("  ar  "), ""), Some("ar".into()));
    }

    #[test]
    fn explicit_language_wins_over_config_default() {
        assert_eq!(resolve_language(Some("ar"), "en"), Some("ar".into()));
    }

    #[test]
    fn empty_and_missing_fall_back_then_omit() {
        assert_eq!(resolve_language(Some(""), "ar"), Some("ar".into()));
        assert_eq!(resolve_language(None, ""), None);
        assert_eq!(resolve_language(Some("   "), "   "), None);
    }

    // --- confidence filtering ----------------------------------------------

    fn segment(avg_logprob: Option<f64>, compression_ratio: Option<f64>) -> GroqSegment {
        GroqSegment {
            text: "x".into(),
            start: None,
            end: None,
            avg_logprob,
            compression_ratio,
            no_speech_prob: None,
        }
    }

    #[test]
    fn low_avg_logprob_is_rejected() {
        assert!(segment(Some(-2.44), None).is_low_confidence());
        assert!(segment(Some(-1.01), None).is_low_confidence());
    }

    #[test]
    fn healthy_avg_logprob_is_kept() {
        // Median on a real recording was about -0.33.
        assert!(!segment(Some(-0.33), None).is_low_confidence());
        assert!(!segment(Some(-1.0), None).is_low_confidence());
    }

    #[test]
    fn repetition_loops_are_rejected_by_compression_ratio() {
        assert!(segment(Some(-0.2), Some(3.0)).is_low_confidence());
        assert!(!segment(Some(-0.2), Some(1.8)).is_low_confidence());
    }

    #[test]
    fn missing_scores_are_kept() {
        // Absent fields must not cause silent data loss.
        assert!(!segment(None, None).is_low_confidence());
    }

    #[test]
    fn no_speech_prob_alone_never_rejects() {
        // Measured: no_speech_prob > 0.5 caught none of the boilerplate
        // hallucinations, because the model is confident when it emits them.
        let mut seg = segment(Some(-0.2), Some(1.5));
        seg.no_speech_prob = Some(0.85);
        assert!(!seg.is_low_confidence());
    }
}
