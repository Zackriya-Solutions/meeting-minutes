// audio/transcription/remote_provider.rs
//
// HTTPS-backed transcription provider. Vendor-neutral: plugs meetily into
// any HTTP ASR backend (WhisperX-compatible worker, cloud GPU, self-hosted).
//
// Wire contract (JSON body, JSON response):
//   Request:  { "audio_base64": "...", "model": "...", "language": "...",
//               "min_speakers": u8?, "max_speakers": u8? }
//   Response: { "segments": [ { start, end, text, speaker? } ], "error": "..."? }
//
// Behaviour:
//   * POST {endpoint_url} with `Authorization: Bearer {token}` if token set.
//   * 4xx/5xx -> TranscriptionError::EngineFailed.
//   * Worker returning `{"error": "..."}` (HTTP 200) -> EngineFailed too.
//   * Empty segments -> empty text, confidence 1.0.
//   * Each segment with `speaker` -> formatted as "SPEAKER_xx: text".
//   * Segments without `speaker` -> concatenated as plain lines.

use super::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use async_trait::async_trait;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Vendor-neutral settings. Caller (Settings UI / fork's config bridge) builds this.
#[derive(Clone, Debug)]
pub struct RemoteProviderConfig {
    pub endpoint_url: String,
    pub bearer_token: String,
    pub model: String,
    pub default_lang: String,
    pub min_speakers: Option<u8>,
    pub max_speakers: Option<u8>,
    pub request_timeout: Duration,
}

impl RemoteProviderConfig {
    /// ponytail: zero-config defaults so the type is constructible without ceremony.
    pub fn with_endpoint(endpoint_url: impl Into<String>) -> Self {
        Self {
            endpoint_url: endpoint_url.into(),
            bearer_token: String::new(),
            model: String::new(),
            default_lang: "en".to_string(),
            min_speakers: None,
            max_speakers: None,
            request_timeout: Duration::from_secs(300),
        }
    }
}

#[derive(Serialize)]
struct RemoteRequest<'a> {
    audio_base64: String,
    model: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_speakers: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_speakers: Option<u8>,
}

#[derive(Deserialize, Debug)]
struct RemoteSegment {
    start: f64,
    end: f64,
    text: String,
    #[serde(default)]
    speaker: Option<String>,
}

#[derive(Deserialize, Debug)]
struct RemoteResponse {
    #[serde(default)]
    segments: Vec<RemoteSegment>,
    #[serde(default)]
    error: Option<String>,
}

pub struct RemoteProvider {
    config: RemoteProviderConfig,
    client: reqwest::Client,
}

impl RemoteProvider {
    pub fn new(config: RemoteProviderConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .build()
            .expect("reqwest::Client::builder with sensible timeout cannot fail");
        Self { config, client }
    }

    /// 44-byte WAV header for 16-bit PCM mono @ `sample_rate`, then LE i16 samples.
    /// Ponytail: hand-rolled because shipping `wav` just to write one static header
    /// is overkill; 44 bytes is not worth a dep.
    pub fn write_wav_pcm16_mono(samples: &[f32], sample_rate: u32, out: &mut Vec<u8>) {
        let byte_rate: u32 = sample_rate * 2; // mono * 16-bit
        let block_align: u16 = 2;
        let data_bytes: u32 = samples.len() as u32 * 2;
        let chunk_size: u32 = 36 + data_bytes;

        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&chunk_size.to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes()); // PCM fmt chunk size
        out.extend_from_slice(&1u16.to_le_bytes()); // audio format = PCM
        out.extend_from_slice(&1u16.to_le_bytes()); // channels = 1
        out.extend_from_slice(&sample_rate.to_le_bytes());
        out.extend_from_slice(&byte_rate.to_le_bytes());
        out.extend_from_slice(&block_align.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_bytes.to_le_bytes());

        for &s in samples {
            let clamped = s.clamp(-1.0, 1.0);
            let i = (clamped * 32767.0) as i16;
            out.extend_from_slice(&i.to_le_bytes());
        }
    }

    fn build_request<'a>(&'a self, wav_bytes: &'a [u8], language: Option<&'a str>) -> RemoteRequest<'a> {
        RemoteRequest {
            audio_base64: base64::engine::general_purpose::STANDARD.encode(wav_bytes),
            model: &self.config.model,
            language,
            min_speakers: self.config.min_speakers,
            max_speakers: self.config.max_speakers,
        }
    }

    /// Format segments. Each segment with `speaker` -> "SPEAKER_xx: text"; else plain line.
    /// Returns "" for empty segments (caller decides confidence default).
    fn format_segments(segments: &[RemoteSegment]) -> String {
        let mut out = String::new();
        for seg in segments {
            let trimmed = seg.text.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(spk) = seg.speaker.as_ref().filter(|s| !s.is_empty()) {
                out.push_str(spk);
                out.push_str(": ");
                out.push_str(trimmed);
            } else {
                out.push_str(trimmed);
            }
            out.push('\n');
        }
        out
    }
}

#[async_trait]
impl TranscriptionProvider for RemoteProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        if self.config.endpoint_url.is_empty() {
            return Err(TranscriptionError::EngineFailed(
                "RemoteProvider: endpoint_url not configured".into(),
            ));
        }

        // ponytail: trust the upstream convention that audio is already 16kHz mono f32.
        // The trait's documented contract already says 16kHz mono; no resample in provider.
        let mut wav = Vec::with_capacity(44 + audio.len() * 2);
        Self::write_wav_pcm16_mono(&audio, 16_000, &mut wav);

        let lang_owned;
        let lang_ref: Option<&str> = match language.as_deref() {
            Some(l) if !l.is_empty() => Some(l),
            _ => {
                // ponytail: prefer explicit `language` arg, fall back to config default.
                if self.config.default_lang.is_empty() {
                    None
                } else {
                    lang_owned = self.config.default_lang.clone();
                    Some(&lang_owned)
                }
            }
        };

        let req = self.build_request(&wav, lang_ref);

        let mut request = self
            .client
            .post(&self.config.endpoint_url)
            .header("Content-Type", "application/json");

        if !self.config.bearer_token.is_empty() {
            request = request.header("Authorization", format!("Bearer {}", self.config.bearer_token));
        }

        let resp = request
            .json(&req)
            .send()
            .await
            .map_err(|e| TranscriptionError::EngineFailed(format!("RemoteProvider: HTTP send: {e}")))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| TranscriptionError::EngineFailed(format!("RemoteProvider: read body: {e}")))?;

        if !status.is_success() {
            return Err(TranscriptionError::EngineFailed(format!(
                "RemoteProvider: HTTP {} - {}",
                status.as_u16(),
                body.chars().take(200).collect::<String>()
            )));
        }

        let parsed: RemoteResponse = serde_json::from_str(&body).map_err(|e| {
            TranscriptionError::EngineFailed(format!("RemoteProvider: parse response: {e}"))
        })?;

        if let Some(err) = parsed.error {
            return Err(TranscriptionError::EngineFailed(format!(
                "RemoteProvider: worker error: {err}"
            )));
        }

        let text = Self::format_segments(&parsed.segments);
        // ponytail: confidence semantics — keep None unless we have an aggregated signal.
        // The simplest defensible default when we don't get a confidence back is 1.0.
        let confidence = Some(1.0_f32);

        Ok(TranscriptResult {
            text,
            confidence,
            is_partial: false,
        })
    }

    async fn is_model_loaded(&self) -> bool {
        !self.config.endpoint_url.is_empty() && !self.config.model.is_empty()
    }

    async fn get_current_model(&self) -> Option<String> {
        if self.config.model.is_empty() {
            None
        } else {
            Some(self.config.model.clone())
        }
    }

    fn provider_name(&self) -> &'static str {
        "RemoteHTTPS"
    }
}

// =============================================================================
// Tests
// =============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    fn sine_1s() -> Vec<f32> {
        (0..16_000).map(|i| (i as f32 / 16_000.0 * 440.0 * 2.0 * std::f32::consts::PI).sin()).collect()
    }

    /// ponytail: assertion-only smoke for the header. No fixture framework.
    #[test]
    fn wav_header_is_44_bytes() {
        let mut buf = Vec::new();
        RemoteProvider::write_wav_pcm16_mono(&sine_1s(), 16_000, &mut buf);
        assert!(buf.len() >= 44);
        assert_eq!(&buf[0..4], b"RIFF");
        assert_eq!(&buf[8..12], b"WAVE");
        assert_eq!(&buf[12..16], b"fmt ");
        assert_eq!(&buf[36..40], b"data");
    }

    #[test]
    fn wav_data_chunk_size_matches_sample_count() {
        let samples = sine_1s();
        let mut buf = Vec::new();
        RemoteProvider::write_wav_pcm16_mono(&samples, 16_000, &mut buf);
        let data_chunk_size = u32::from_le_bytes([buf[40], buf[41], buf[42], buf[43]]);
        assert_eq!(data_chunk_size as usize, samples.len() * 2);
        assert_eq!(buf.len(), 44 + samples.len() * 2);
    }

    #[test]
    fn empty_audio_produces_minimal_wav() {
        let mut buf = Vec::new();
        RemoteProvider::write_wav_pcm16_mono(&[], 16_000, &mut buf);
        assert_eq!(buf.len(), 44);
        let chunk_size = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
        assert_eq!(chunk_size, 36);
    }

    #[test]
    fn format_segments_with_speaker_prefix() {
        let segs = vec![
            RemoteSegment { start: 0.0, end: 1.0, text: " hello".into(), speaker: Some("SPEAKER_00".into()) },
            RemoteSegment { start: 1.0, end: 2.0, text: "world".into(), speaker: None },
        ];
        let text = RemoteProvider::format_segments(&segs);
        assert_eq!(text, "SPEAKER_00: hello\nworld\n");
    }

    #[test]
    fn format_segments_skips_empty_text() {
        let segs = vec![
            RemoteSegment { start: 0.0, end: 1.0, text: "   ".into(), speaker: Some("SPEAKER_00".into()) },
            RemoteSegment { start: 1.0, end: 2.0, text: "hi".into(), speaker: None },
        ];
        let text = RemoteProvider::format_segments(&segs);
        assert_eq!(text, "hi\n");
    }

    #[tokio::test]
    async fn empty_endpoint_returns_engine_failed() {
        let cfg = RemoteProviderConfig::with_endpoint("");
        let p = RemoteProvider::new(cfg);
        let res = p.transcribe(sine_1s(), None).await;
        assert!(matches!(res, Err(TranscriptionError::EngineFailed(_))));
    }

    /// Wire-format integration test: spin up a one-shot TCP listener that
    /// replies with a pre-canned JSON body, no third-party mock dep needed.
    async fn one_shot_http_server(body: &'static str, status_line: &'static str) -> (String, oneshot::Sender<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{addr}/");
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            tokio::select! {
                _ = async {
                    if let Ok((mut sock, _)) = listener.accept().await {
                        let mut buf = [0u8; 4096];
                        let _ = sock.read(&mut buf).await;
                        let payload = format!(
                            "{status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = sock.write_all(payload.as_bytes()).await;
                        let _ = sock.shutdown().await;
                    }
                } => {}
                _ = stop_rx => {}
            }
        });
        // park server handle so it drops when this fn returns
        std::mem::forget(server);
        (url, stop_tx)
    }

    #[tokio::test]
    async fn happy_path_two_segments_with_speaker() {
        let body = r#"{"segments":[
            {"start":0.0,"end":1.0,"text":" hello","speaker":"SPEAKER_00"},
            {"start":1.0,"end":2.0,"text":"hi there","speaker":"SPEAKER_01"}
        ]}"#;
        let (url, _stop) = one_shot_http_server(body, "HTTP/1.1 200 OK").await;
        let cfg = RemoteProviderConfig {
            endpoint_url: url,
            bearer_token: "tok".into(),
            model: "whisperx".into(),
            default_lang: "en".into(),
            min_speakers: None,
            max_speakers: None,
            request_timeout: Duration::from_secs(10),
        };
        let p = RemoteProvider::new(cfg);
        let res = p.transcribe(sine_1s(), Some("en".into())).await.expect("transcribe");
        assert_eq!(res.text, "SPEAKER_00: hello\nSPEAKER_01: hi there\n");
        assert!(!res.is_partial);
    }

    #[tokio::test]
    async fn empty_segments_returns_empty_text() {
        let body = r#"{"segments":[]}"#;
        let (url, _stop) = one_shot_http_server(body, "HTTP/1.1 200 OK").await;
        let cfg = RemoteProviderConfig::with_endpoint(url);
        let p = RemoteProvider::new(cfg);
        let res = p.transcribe(sine_1s(), None).await.expect("transcribe");
        assert_eq!(res.text, "");
        assert_eq!(res.confidence, Some(1.0));
    }

    #[tokio::test]
    async fn http_4xx_returns_engine_failed() {
        let body = "upstream blew up";
        let (url, _stop) = one_shot_http_server(body, "HTTP/1.1 500 Internal Server Error").await;
        let cfg = RemoteProviderConfig::with_endpoint(url);
        let p = RemoteProvider::new(cfg);
        let res = p.transcribe(sine_1s(), None).await;
        match res {
            Err(TranscriptionError::EngineFailed(msg)) => assert!(msg.contains("500")),
            other => panic!("expected EngineFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn worker_error_field_returns_engine_failed() {
        let body = r#"{"error":"model not found"}"#;
        let (url, _stop) = one_shot_http_server(body, "HTTP/1.1 200 OK").await;
        let cfg = RemoteProviderConfig::with_endpoint(url);
        let p = RemoteProvider::new(cfg);
        let res = p.transcribe(sine_1s(), None).await;
        match res {
            Err(TranscriptionError::EngineFailed(msg)) => assert!(msg.contains("model not found")),
            other => panic!("expected EngineFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unparseable_response_returns_engine_failed() {
        let body = "not json";
        let (url, _stop) = one_shot_http_server(body, "HTTP/1.1 200 OK").await;
        let cfg = RemoteProviderConfig::with_endpoint(url);
        let p = RemoteProvider::new(cfg);
        let res = p.transcribe(sine_1s(), None).await;
        assert!(matches!(res, Err(TranscriptionError::EngineFailed(_))));
    }
}
