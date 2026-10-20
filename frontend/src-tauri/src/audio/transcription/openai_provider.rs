// audio/transcription/openai_provider.rs
//
// OpenAI-compatible transcription provider. Works against any server
// implementing the `/audio/transcriptions` endpoint (api.openai.com or a
// self-hosted compatible server), configured via endpoint + model + api key.

use super::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Config for a custom OpenAI-compatible transcription endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomTranscriptionConfig {
    /// Base URL of the OpenAI-compatible API (e.g. "https://api.openai.com/v1")
    pub endpoint: String,
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
    pub model: String,
}

pub struct OpenAIProvider {
    client: reqwest::Client,
    config: CustomTranscriptionConfig,
    loaded: RwLock<bool>,
}

impl OpenAIProvider {
    pub fn new(config: CustomTranscriptionConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
            loaded: RwLock::new(true),
        }
    }
}

#[async_trait]
impl TranscriptionProvider for OpenAIProvider {
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

        let wav_bytes = encode_wav_16khz_mono(&audio);
        let endpoint = format!(
            "{}/audio/transcriptions",
            self.config.endpoint.trim_end_matches('/')
        );

        let part = reqwest::multipart::Part::bytes(wav_bytes)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| TranscriptionError::EngineFailed(e.to_string()))?;

        let mut form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("model", self.config.model.clone());

        if let Some(lang) = language {
            form = form.text("language", lang);
        }

        let mut request = self.client.post(&endpoint).multipart(form);
        if let Some(key) = &self.config.api_key {
            if !key.is_empty() {
                request = request.bearer_auth(key);
            }
        }

        let response = request
            .send()
            .await
            .map_err(|e| TranscriptionError::EngineFailed(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(TranscriptionError::EngineFailed(format!(
                "HTTP {}: {}",
                status, body
            )));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| TranscriptionError::EngineFailed(e.to_string()))?;

        let text = body
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();

        Ok(TranscriptResult {
            text,
            confidence: None,
            is_partial: false,
        })
    }

    async fn is_model_loaded(&self) -> bool {
        *self.loaded.read().await
    }

    async fn get_current_model(&self) -> Option<String> {
        Some(self.config.model.clone())
    }

    fn provider_name(&self) -> &'static str {
        "OpenAI-Compatible"
    }
}

/// Encode 16kHz mono f32 PCM samples as a 16-bit PCM WAV byte buffer.
fn encode_wav_16khz_mono(samples: &[f32]) -> Vec<u8> {
    const SAMPLE_RATE: u32 = 16_000;
    const BITS_PER_SAMPLE: u16 = 16;
    const CHANNELS: u16 = 1;

    let data_len = (samples.len() * 2) as u32;
    let byte_rate = SAMPLE_RATE * CHANNELS as u32 * (BITS_PER_SAMPLE as u32 / 8);
    let block_align = CHANNELS * (BITS_PER_SAMPLE / 8);

    let mut wav = Vec::with_capacity(44 + data_len as usize);

    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&CHANNELS.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());

    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for &sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let i16_sample = (clamped * i16::MAX as f32) as i16;
        wav.extend_from_slice(&i16_sample.to_le_bytes());
    }

    wav
}
