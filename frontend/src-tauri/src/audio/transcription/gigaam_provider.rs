// audio/transcription/gigaam_provider.rs
//
// GigaAM transcription provider — thin adapter over the process-global GigaAM engine
// (crate::gigaam_engine). Unlike Whisper/Parakeet (which hold an Arc<Engine>), GigaAM
// uses a global model, so this provider is stateless.

use super::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use async_trait::async_trait;

/// GigaAM v3 e2e-CTC provider (Russian ASR, punctuated output).
pub struct GigaamProvider;

#[async_trait]
impl TranscriptionProvider for GigaamProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        _language: Option<String>, // GigaAM is Russian-only; language hint is ignored.
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        match crate::gigaam_engine::transcribe(audio).await {
            Some(Ok(text)) => Ok(TranscriptResult {
                text: text.trim().to_string(),
                confidence: None,
                is_partial: false,
            }),
            Some(Err(e)) => Err(TranscriptionError::EngineFailed(e)),
            None => Err(TranscriptionError::EngineFailed(
                "GigaAM model not loaded".to_string(),
            )),
        }
    }

    async fn is_model_loaded(&self) -> bool {
        crate::gigaam_engine::is_loaded()
    }

    async fn get_current_model(&self) -> Option<String> {
        crate::gigaam_engine::is_loaded().then(crate::gigaam_engine::model_label)
    }

    fn provider_name(&self) -> &'static str {
        "GigaAM"
    }
}
