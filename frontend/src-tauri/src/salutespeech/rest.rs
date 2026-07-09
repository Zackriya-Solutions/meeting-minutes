//! `SaluteSpeechProvider` — a [`TranscriptionProvider`] backed by SaluteSpeech's
//! synchronous HTTP recognize API. One speech segment → one POST → one transcript,
//! so it works uniformly for live recording, file import, and re-transcription.
//!
//! Audio arrives as 16 kHz mono f32 (the worker resamples before calling us); we send
//! it as 16-bit little-endian PCM. Speaker labels are NOT part of the sync response —
//! speaker attribution is handled by the app's local diarization / channel tagging.

use async_trait::async_trait;

use super::{map_language, SaluteSpeechConfig};
use crate::audio::transcription::provider::{
    TranscriptionError, TranscriptionProvider, TranscriptResult,
};
use super::auth::SaluteSpeechAuth;

/// The worker delivers 16 kHz mono audio; the recognize `Content-Type` fixes the rate.
const RECOGNIZE_CONTENT_TYPE: &str = "audio/x-pcm;bit=16;rate=16000";
const MIN_SAMPLES: usize = 1_600; // 100 ms @ 16 kHz — below this, don't bother the API.

pub struct SaluteSpeechProvider {
    auth: SaluteSpeechAuth,
    client: reqwest::Client,
    recognize_url: String,
    model: String,
}

impl SaluteSpeechProvider {
    pub fn new(cfg: SaluteSpeechConfig) -> Self {
        Self {
            auth: SaluteSpeechAuth::new(cfg.auth_key, cfg.oauth_url, cfg.scope),
            client: reqwest::Client::new(),
            recognize_url: cfg.recognize_url,
            model: cfg.model,
        }
    }
}

#[async_trait]
impl TranscriptionProvider for SaluteSpeechProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> Result<TranscriptResult, TranscriptionError> {
        if audio.len() < MIN_SAMPLES {
            return Err(TranscriptionError::AudioTooShort {
                samples: audio.len(),
                minimum: MIN_SAMPLES,
            });
        }

        let token = self
            .auth
            .access_token()
            .await
            .map_err(TranscriptionError::EngineFailed)?;

        let lang = map_language(language);

        // f32 [-1.0, 1.0] @ 16 kHz mono → 16-bit LE PCM.
        let mut pcm = Vec::with_capacity(audio.len() * 2);
        for s in audio {
            let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            pcm.extend_from_slice(&v.to_le_bytes());
        }

        let mut query: Vec<(&str, &str)> = vec![("language", lang.as_str())];
        if !self.model.trim().is_empty() {
            query.push(("model", self.model.as_str()));
        }

        let resp = self
            .client
            .post(&self.recognize_url)
            .query(&query)
            .bearer_auth(&token)
            .header(reqwest::header::CONTENT_TYPE, RECOGNIZE_CONTENT_TYPE)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::USER_AGENT, "GigaChat-Meetily")
            .body(pcm)
            .send()
            .await
            .map_err(|e| {
                TranscriptionError::EngineFailed(format!("salutespeech recognize request failed: {e}"))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(TranscriptionError::EngineFailed(format!(
                "salutespeech recognize error {status}: {}",
                text.chars().take(300).collect::<String>()
            )));
        }

        let v: serde_json::Value = resp.json().await.map_err(|e| {
            TranscriptionError::EngineFailed(format!("salutespeech recognize parse failed: {e}"))
        })?;

        // Response shape: { "result": ["text", …], "status": 200 }
        let text = v
            .get("result")
            .and_then(|r| r.as_array())
            .and_then(|arr| arr.first())
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        Ok(TranscriptResult {
            text,
            confidence: None,
            is_partial: false,
        })
    }

    async fn is_model_loaded(&self) -> bool {
        // Cloud — always "loaded" once configured (construction requires an auth key).
        true
    }

    async fn get_current_model(&self) -> Option<String> {
        Some(format!("salutespeech/{}", self.model))
    }

    fn provider_name(&self) -> &'static str {
        "SaluteSpeech"
    }
}
