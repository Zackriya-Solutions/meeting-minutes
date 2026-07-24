use super::provider::{TranscriptResult, TranscriptionError, TranscriptionProvider};
use async_trait::async_trait;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct OpenAiCompatibleConfig {
    pub endpoint: String,
    pub api_key: Option<String>,
    pub model: String,
    pub request_timeout: Duration,
}

pub struct OpenAiCompatibleProvider {
    config: OpenAiCompatibleConfig,
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    #[serde(default)]
    text: String,
}

impl OpenAiCompatibleProvider {
    pub fn new(config: OpenAiCompatibleConfig) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .build()
            .map_err(|error| format!("Failed to create ASR HTTP client: {error}"))?;
        Ok(Self { config, client })
    }

    fn transcription_url(endpoint: &str) -> String {
        let endpoint = endpoint.trim().trim_end_matches('/');
        if endpoint.ends_with("/audio/transcriptions") {
            endpoint.to_string()
        } else if endpoint.ends_with("/v1") {
            format!("{endpoint}/audio/transcriptions")
        } else {
            format!("{endpoint}/v1/audio/transcriptions")
        }
    }

    fn wav_pcm16_mono(samples: &[f32], sample_rate: u32) -> Vec<u8> {
        let data_size = (samples.len() * 2) as u32;
        let mut wav = Vec::with_capacity(44 + data_size as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_size).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_size.to_le_bytes());
        for sample in samples {
            let pcm = ((*sample).clamp(-1.0, 1.0) * 32767.0).round() as i16;
            wav.extend_from_slice(&pcm.to_le_bytes());
        }
        wav
    }
}

#[async_trait]
impl TranscriptionProvider for OpenAiCompatibleProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> Result<TranscriptResult, TranscriptionError> {
        if self.config.endpoint.trim().is_empty() {
            return Err(TranscriptionError::EngineFailed(
                "OpenAI-compatible ASR endpoint is not configured".to_string(),
            ));
        }
        if self.config.model.trim().is_empty() {
            return Err(TranscriptionError::EngineFailed(
                "OpenAI-compatible ASR model is not configured".to_string(),
            ));
        }

        let wav = Self::wav_pcm16_mono(&audio, 16_000);
        let file = Part::bytes(wav)
            .file_name("meetily-chunk.wav")
            .mime_str("audio/wav")
            .map_err(|error| TranscriptionError::EngineFailed(error.to_string()))?;
        let mut form = Form::new()
            .part("file", file)
            .text("model", self.config.model.clone())
            .text("response_format", "json");
        if let Some(language) = language.filter(|value| !value.is_empty() && value != "auto") {
            form = form.text("language", language);
        }

        let mut request = self
            .client
            .post(Self::transcription_url(&self.config.endpoint))
            .multipart(form);
        if let Some(api_key) = self
            .config
            .api_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            request = request.bearer_auth(api_key);
        }

        let response = request.send().await.map_err(|error| {
            TranscriptionError::EngineFailed(format!("ASR request failed: {error}"))
        })?;
        let status = response.status();
        let body = response.text().await.map_err(|error| {
            TranscriptionError::EngineFailed(format!("Failed to read ASR response: {error}"))
        })?;
        if !status.is_success() {
            let message: String = body.chars().take(300).collect();
            return Err(TranscriptionError::EngineFailed(format!(
                "ASR service returned HTTP {}: {}",
                status.as_u16(),
                message
            )));
        }

        let response: TranscriptionResponse = serde_json::from_str(&body).map_err(|error| {
            TranscriptionError::EngineFailed(format!(
                "ASR service returned an invalid transcription response: {error}"
            ))
        })?;
        Ok(TranscriptResult {
            text: response.text,
            confidence: None,
            is_partial: false,
        })
    }

    async fn is_model_loaded(&self) -> bool {
        !self.config.endpoint.trim().is_empty() && !self.config.model.trim().is_empty()
    }

    async fn get_current_model(&self) -> Option<String> {
        (!self.config.model.trim().is_empty()).then(|| self.config.model.clone())
    }

    fn provider_name(&self) -> &'static str {
        "OpenAI-compatible ASR"
    }
}

#[cfg(test)]
mod tests {
    use super::{OpenAiCompatibleConfig, OpenAiCompatibleProvider};
    use crate::audio::transcription::provider::TranscriptionProvider;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn resolves_base_and_full_transcription_urls() {
        assert_eq!(
            OpenAiCompatibleProvider::transcription_url("https://api.openai.com"),
            "https://api.openai.com/v1/audio/transcriptions"
        );
        assert_eq!(
            OpenAiCompatibleProvider::transcription_url("http://localhost:8000/v1/"),
            "http://localhost:8000/v1/audio/transcriptions"
        );
        assert_eq!(
            OpenAiCompatibleProvider::transcription_url(
                "https://example.test/v1/audio/transcriptions"
            ),
            "https://example.test/v1/audio/transcriptions"
        );
    }

    #[test]
    fn encodes_valid_pcm_wav_header() {
        let wav = OpenAiCompatibleProvider::wav_pcm16_mono(&[0.0, 1.0, -1.0], 16_000);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(wav.len(), 50);
    }

    #[tokio::test]
    async fn sends_openai_compatible_multipart_request() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let count = socket.read(&mut buffer).await.unwrap();
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..count]);
                let header_end = request
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .map(|position| position + 4);
                let Some(header_end) = header_end else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap();
                if request.len() >= header_end + content_length {
                    break;
                }
            }

            let request_text = String::from_utf8_lossy(&request);
            assert!(request_text.starts_with("POST /v1/audio/transcriptions HTTP/1.1"));
            assert!(request_text.contains("authorization: Bearer test-key"));
            assert!(request_text.contains("name=\"file\""));
            assert!(request_text.contains("name=\"model\""));
            assert!(request_text.contains("whisper-test"));
            assert!(request_text.contains("name=\"language\""));
            assert!(request_text.contains("en"));

            let body = r#"{"text":"mocked transcript"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let provider = OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
            endpoint: format!("http://{address}"),
            api_key: Some("test-key".to_string()),
            model: "whisper-test".to_string(),
            request_timeout: Duration::from_secs(5),
        })
        .unwrap();
        let result = provider
            .transcribe(vec![0.0; 320], Some("en".to_string()))
            .await
            .unwrap();

        assert_eq!(result.text, "mocked transcript");
        server.await.unwrap();
    }
}
