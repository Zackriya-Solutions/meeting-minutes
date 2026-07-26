// audio/transcription/provider.rs
//
// Defines the unified TranscriptionProvider trait and common types for all
// transcription engines (Whisper, Parakeet, future providers).

use async_trait::async_trait;

// ============================================================================
// TRANSCRIPTION PROVIDER TRAIT & ERROR TYPES
// ============================================================================

/// Granular error types for transcription operations
#[derive(Debug, Clone)]
pub enum TranscriptionError {
    ModelNotLoaded,
    AudioTooShort { samples: usize, minimum: usize },
    EngineFailed(String),
    UnsupportedLanguage(String),
}

impl std::fmt::Display for TranscriptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModelNotLoaded => write!(f, "No transcription model is loaded"),
            Self::AudioTooShort { samples, minimum } => write!(
                f,
                "Audio too short: {} samples (minimum {})",
                samples, minimum
            ),
            Self::EngineFailed(msg) => write!(f, "Transcription engine failed: {}", msg),
            Self::UnsupportedLanguage(lang) => {
                write!(f, "Language '{}' is not supported by this provider", lang)
            }
        }
    }
}

impl std::error::Error for TranscriptionError {}

/// Unified transcription result across all providers
#[derive(Debug, Clone)]
pub struct TranscriptResult {
    pub text: String,
    pub confidence: Option<f32>, // None if provider doesn't support confidence scores
    pub is_partial: bool,
}

/// Trait for transcription providers (Whisper, Parakeet, future providers)
#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    /// Transcribe audio samples to text
    ///
    /// # Arguments
    /// * `audio` - Audio samples (16kHz mono, f32 format)
    /// * `language` - Optional language hint (e.g., "en", "es", "fr")
    ///
    /// # Returns
    /// * `TranscriptResult` with text, optional confidence, and partial flag
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError>;

    /// Check if a model is currently loaded
    async fn is_model_loaded(&self) -> bool;

    /// Get the name of the currently loaded model
    async fn get_current_model(&self) -> Option<String>;

    /// Get the provider name (for logging/debugging)
    fn provider_name(&self) -> &'static str;

    /// Whether this provider decodes a continuous stream one step at a time.
    ///
    /// A streaming provider carries encoder state across steps, so the pipeline can
    /// hand it every sample as it arrives and get text back without waiting for the
    /// speaker to pause. A provider that answers `false` needs whole utterances,
    /// which is why VAD segmentation still exists.
    fn supports_streaming(&self) -> bool {
        false
    }

    /// Decode exactly one step of a continuous stream.
    ///
    /// `audio` must be exactly [`STREAM_STEP_SAMPLES`] long. This is a hard
    /// requirement, not a hint: the underlying encoder advances its cursor by one
    /// step per call and buffers anything beyond that without ever catching up, so
    /// a longer buffer loses audio silently. Measured cost of getting this wrong:
    /// 62.6% word error rate against 5.9% when the size is right.
    ///
    /// The returned piece is **verbatim** - leading and trailing spaces are
    /// significant, because they are what marks the start of a word. Callers
    /// concatenate pieces with nothing between them and trim once at the end.
    /// An empty piece is normal and means the step decoded to nothing, which is
    /// what silence produces.
    async fn transcribe_step(
        &self,
        audio: Vec<f32>,
    ) -> std::result::Result<String, TranscriptionError> {
        let _ = audio;
        Err(TranscriptionError::EngineFailed(format!(
            "{} does not decode streams a step at a time",
            self.provider_name()
        )))
    }

    /// Discard everything the decoder remembers, so the next step starts fresh.
    ///
    /// State is deliberately kept *within* a recording - that continuity is what lets
    /// a word split across two steps come out whole. But it must not survive between
    /// recordings: without this the first words of a meeting are conditioned on the
    /// last sentence of the previous one, which is a different conversation.
    ///
    /// A provider that does not stream has no state to clear and does nothing.
    async fn reset_stream(&self) -> std::result::Result<(), TranscriptionError> {
        Ok(())
    }
}

/// Samples in one streaming step: 560 ms at 16 kHz.
///
/// Fixed by the model, not chosen: Nemotron's cache-aware encoder consumes 56 mel
/// frames per step at a 160-sample hop. Feeding a different amount is the audio
/// loss described on [`TranscriptionProvider::transcribe_step`].
pub const STREAM_STEP_SAMPLES: usize = 8_960;
