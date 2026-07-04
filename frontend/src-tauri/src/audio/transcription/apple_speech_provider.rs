// audio/transcription/apple_speech_provider.rs
//
// Apple Speech transcription provider implementation.
// Follows the same pattern as whisper_provider.rs and parakeet_provider.rs.

use super::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use async_trait::async_trait;
use log::info;
use std::sync::Arc;

/// Apple Speech transcription provider (wraps AppleSpeechEngine)
pub struct AppleSpeechProvider {
    engine: Arc<crate::apple_speech_engine::AppleSpeechEngine>,
}

impl AppleSpeechProvider {
    pub fn new(engine: Arc<crate::apple_speech_engine::AppleSpeechEngine>) -> Self {
        Self { engine }
    }
}

#[async_trait]
impl TranscriptionProvider for AppleSpeechProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        _language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        // Language is set at engine init time via locale, not per-transcription call.
        // The recognizer uses the locale it was initialized with.
        match self.engine.transcribe_audio(audio).await {
            Ok((text, confidence, is_partial)) => Ok(TranscriptResult {
                text: text.trim().to_string(),
                confidence,
                is_partial,
            }),
            Err(e) => Err(TranscriptionError::EngineFailed(e.to_string())),
        }
    }

    async fn is_model_loaded(&self) -> bool {
        self.engine.is_model_loaded().await
    }

    async fn get_current_model(&self) -> Option<String> {
        self.engine.get_current_model().await
    }

    fn provider_name(&self) -> &'static str {
        "Apple Speech"
    }
}
