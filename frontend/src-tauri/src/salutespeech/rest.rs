//! `SaluteSpeechProvider` — a [`TranscriptionProvider`] backed by SaluteSpeech's
//! synchronous HTTP recognize API. One speech segment → one POST → one transcript,
//! so it works uniformly for live recording, file import, and re-transcription.
//!
//! Audio arrives as 16 kHz mono f32 (the worker resamples before calling us); we send
//! it as 16-bit little-endian PCM. Speaker labels are NOT part of the sync response —
//! speaker attribution is handled by the app's local diarization / channel tagging.

use async_trait::async_trait;
use std::time::Duration;

use super::auth::SaluteSpeechAuth;
use super::{map_language, SaluteSpeechConfig};
use crate::audio::transcription::provider::{
    TranscriptResult, TranscriptionError, TranscriptionProvider,
};

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
            // reqwest has no total request timeout by default. A dead VPN/proxy used to hold
            // the only ordered live worker forever, making every later segment appear stuck.
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(45))
                .build()
                .unwrap_or_else(|error| {
                    log::warn!("Could not configure SaluteSpeech request timeouts: {error}");
                    reqwest::Client::new()
                }),
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
                TranscriptionError::EngineFailed(format!(
                    "salutespeech recognize request failed: {e}"
                ))
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

#[cfg(test)]
mod live_load_tests {
    use super::*;
    use crate::audio::common::{process_bounded_ordered, CLOUD_TRANSCRIPTION_MAX_CONCURRENCY};
    use crate::audio::transcription::TranscriptionProvider;

    #[tokio::test]
    #[ignore = "requires managed gateway credentials and SALUTESPEECH_LOAD_TEST_PCM"]
    async fn managed_gateway_handles_parallel_recognition() {
        let path = std::env::var("SALUTESPEECH_LOAD_TEST_PCM")
            .expect("SALUTESPEECH_LOAD_TEST_PCM must point to 16 kHz mono f32le PCM");
        let bytes = std::fs::read(path).unwrap();
        let audio: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        assert!(audio.len() >= 1_600);

        let (token, base) = crate::gateway_identity::install_token().await.unwrap();
        let provider = SaluteSpeechProvider::new(super::super::SaluteSpeechConfig {
            auth_key: token,
            scope: None,
            oauth_url: format!("{}/salutespeech/token", base.trim_end_matches('/')),
            recognize_url: super::super::DEFAULT_RECOGNIZE_URL.to_string(),
            model: super::super::DEFAULT_MODEL.to_string(),
        });
        let sequential_started = std::time::Instant::now();
        let sequential = process_bounded_ordered(
            vec![audio.clone(); CLOUD_TRANSCRIPTION_MAX_CONCURRENCY],
            1,
            |_, samples| provider.transcribe(samples, Some("ru".to_string())),
        )
        .await
        .unwrap();
        let sequential_elapsed = sequential_started.elapsed();

        let parallel_started = std::time::Instant::now();
        let parallel = process_bounded_ordered(
            vec![audio; CLOUD_TRANSCRIPTION_MAX_CONCURRENCY],
            CLOUD_TRANSCRIPTION_MAX_CONCURRENCY,
            |_, samples| provider.transcribe(samples, Some("ru".to_string())),
        )
        .await
        .unwrap();
        let parallel_elapsed = parallel_started.elapsed();

        assert_eq!(parallel.len(), CLOUD_TRANSCRIPTION_MAX_CONCURRENCY);
        assert!(sequential
            .iter()
            .all(|result| !result.text.trim().is_empty()));
        assert!(parallel.iter().all(|result| !result.text.trim().is_empty()));
        eprintln!(
            "requests={} sequential_ms={} parallel_ms={} speedup={:.2}x",
            parallel.len(),
            sequential_elapsed.as_millis(),
            parallel_elapsed.as_millis(),
            sequential_elapsed.as_secs_f64() / parallel_elapsed.as_secs_f64()
        );
    }
}
