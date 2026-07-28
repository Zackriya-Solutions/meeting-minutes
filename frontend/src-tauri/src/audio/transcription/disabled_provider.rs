// audio/transcription/disabled_provider.rs
//
// No-op transcription provider. Selection = user opts out of live
// transcription entirely (audio still gets recorded to disk; the
// user can post-hoc transcribe the WAV). Solves #519, #338.
//
// All trait methods are safe:
//   * `transcribe`   -> empty result
//   * `is_model_loaded` -> true (so per-segment transcoding path
//     treats it as "no work to do" instead of hot-loading a model)
//   * `get_current_model` -> Some("disabled")
//   * `provider_name` -> "Disabled (recording only)"

use super::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};

#[derive(Default, Debug)]
pub struct DisabledProvider;

#[async_trait::async_trait]
impl TranscriptionProvider for DisabledProvider {
    async fn transcribe(
        &self,
        _audio: Vec<f32>,
        _language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        Ok(TranscriptResult {
            text: String::new(),
            confidence: Some(1.0),
            is_partial: false,
        })
    }

    async fn is_model_loaded(&self) -> bool {
        true
    }

    async fn get_current_model(&self) -> Option<String> {
        Some("disabled".to_string())
    }

    fn provider_name(&self) -> &'static str {
        "Disabled (recording only)"
    }
}