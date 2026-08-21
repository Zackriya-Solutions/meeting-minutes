// audio/transcription/sarvam_provider.rs
//
// Sarvam AI speech-to-text provider — an *online* transcription backend.
//
// This is the first cloud provider wired through the generic
// `TranscriptionProvider` trait, giving Meetily an alternative to the local
// Whisper/Parakeet engines when the machine is too weak to transcribe in
// real time without stutter (or when higher accuracy for Indic languages is
// wanted). Each audio chunk handed to us by the transcription worker is
// encoded as a 16 kHz mono WAV and POSTed to Sarvam's REST endpoint.
//
// Docs: https://docs.sarvam.ai/api-reference/speech-to-text/transcribe

use super::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use async_trait::async_trait;
use log::{info, warn};
use serde::Deserialize;
use std::time::Duration;

const SARVAM_ENDPOINT: &str = "https://api.sarvam.ai/speech-to-text";
const DEFAULT_MODEL: &str = "saaras:v3";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Sarvam's REST endpoint targets clips under ~30s; below this many samples a
/// request is wasteful/noisy, so we skip it (mirrors the other providers'
/// "audio too short" guard). 100ms at 16kHz.
const MIN_SAMPLES: usize = 1600;

#[derive(Debug, Deserialize)]
struct SarvamResponse {
    #[serde(default)]
    transcript: String,
    #[allow(dead_code)]
    #[serde(default)]
    language_code: Option<String>,
}

/// Online transcription via the Sarvam AI REST API.
pub struct SarvamProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl SarvamProvider {
    /// `model` is the Sarvam model id (e.g. "saaras:v3" / "saaras:v4"); falls
    /// back to the recommended default when empty.
    pub fn new(api_key: String, model: String) -> Self {
        let model = if model.trim().is_empty() {
            DEFAULT_MODEL.to_string()
        } else {
            model
        };
        Self {
            api_key,
            model,
            client: reqwest::Client::new(),
        }
    }

    /// Maps Meetily's short language hint (e.g. "en", "hi") to Sarvam's BCP-47
    /// `language_code` (e.g. "en-IN", "hi-IN"). Anything unknown / unset maps to
    /// "unknown", letting Sarvam auto-detect.
    fn sarvam_language_code(language: &Option<String>) -> String {
        let lang = match language {
            Some(l) if !l.trim().is_empty() => l.trim().to_lowercase(),
            _ => return "unknown".to_string(),
        };

        // Already a BCP-47 code like "en-IN"? pass through.
        if lang.contains('-') {
            return lang
                .split_once('-')
                .map(|(a, b)| format!("{}-{}", a, b.to_uppercase()))
                .unwrap_or(lang);
        }

        match lang.as_str() {
            "en" => "en-IN",
            "hi" => "hi-IN",
            "bn" => "bn-IN",
            "kn" => "kn-IN",
            "ml" => "ml-IN",
            "mr" => "mr-IN",
            "od" | "or" => "od-IN",
            "pa" => "pa-IN",
            "ta" => "ta-IN",
            "te" => "te-IN",
            "gu" => "gu-IN",
            "as" => "as-IN",
            "ur" => "ur-IN",
            "ne" => "ne-IN",
            "kok" => "kok-IN",
            "ks" => "ks-IN",
            "sd" => "sd-IN",
            "sa" => "sa-IN",
            "sat" => "sat-IN",
            "mni" => "mni-IN",
            "brx" => "brx-IN",
            "mai" => "mai-IN",
            "doi" => "doi-IN",
            _ => "unknown",
        }
        .to_string()
    }
}

/// Encodes 16kHz mono f32 samples (range roughly [-1.0, 1.0]) as a 16-bit PCM
/// WAV byte buffer. Manual RIFF construction avoids pulling in an encoder crate
/// (the project deliberately dropped `hound`).
fn encode_wav_16k_mono(samples: &[f32]) -> Vec<u8> {
    const SAMPLE_RATE: u32 = 16_000;
    const CHANNELS: u16 = 1;
    const BITS_PER_SAMPLE: u16 = 16;

    let byte_rate = SAMPLE_RATE * CHANNELS as u32 * (BITS_PER_SAMPLE as u32 / 8);
    let block_align = CHANNELS * (BITS_PER_SAMPLE / 8);
    let data_len = (samples.len() * 2) as u32;
    let riff_len = 36 + data_len;

    let mut buf = Vec::with_capacity(44 + data_len as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&riff_len.to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    // fmt chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // PCM fmt chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // audio format = PCM
    buf.extend_from_slice(&CHANNELS.to_le_bytes());
    buf.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());

    // data chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let val = (clamped * i16::MAX as f32) as i16;
        buf.extend_from_slice(&val.to_le_bytes());
    }

    buf
}

#[async_trait]
impl TranscriptionProvider for SarvamProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        if audio.len() < MIN_SAMPLES {
            return Err(TranscriptionError::AudioTooShort {
                samples: audio.len(),
                minimum: MIN_SAMPLES,
            });
        }

        let wav = encode_wav_16k_mono(&audio);
        let language_code = Self::sarvam_language_code(&language);

        let file_part = reqwest::multipart::Part::bytes(wav)
            .file_name("chunk.wav")
            .mime_str("audio/wav")
            .map_err(|e| TranscriptionError::EngineFailed(format!("multipart build failed: {}", e)))?;

        let form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("model", self.model.clone())
            .text("language_code", language_code);

        info!(
            "Sarvam: transcribing {} samples with model '{}'",
            audio.len(),
            self.model
        );

        let response = self
            .client
            .post(SARVAM_ENDPOINT)
            .header("api-subscription-key", &self.api_key)
            .multipart(form)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|e| TranscriptionError::EngineFailed(format!("request to Sarvam failed: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                return Err(TranscriptionError::EngineFailed(format!(
                    "Sarvam rejected the API key (HTTP {}). Check the key in Transcript settings.",
                    status
                )));
            }
            return Err(TranscriptionError::EngineFailed(format!(
                "Sarvam API error (HTTP {}): {}",
                status, body
            )));
        }

        let parsed: SarvamResponse = response
            .json()
            .await
            .map_err(|e| TranscriptionError::EngineFailed(format!("failed to parse Sarvam response: {}", e)))?;

        Ok(TranscriptResult {
            text: parsed.transcript.trim().to_string(),
            // Sarvam's REST response doesn't include a per-transcript confidence.
            confidence: None,
            is_partial: false,
        })
    }

    async fn is_model_loaded(&self) -> bool {
        // No local model to load — the provider is "ready" as long as a key is set.
        if self.api_key.trim().is_empty() {
            warn!("Sarvam provider has no API key configured");
            return false;
        }
        true
    }

    async fn get_current_model(&self) -> Option<String> {
        Some(self.model.clone())
    }

    fn provider_name(&self) -> &'static str {
        "Sarvam"
    }
}
