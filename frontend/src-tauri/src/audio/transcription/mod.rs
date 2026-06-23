// audio/transcription/mod.rs
//
// Transcription module: Provider abstraction, engine management, and worker pool.

pub mod engine;
pub mod parakeet_provider;
pub mod provider;
pub mod whisper_provider;
pub mod worker;
#[cfg(target_os = "macos")]
pub mod apple_speech_provider;

// Re-export commonly used types
pub use engine::{
    get_or_init_transcription_engine, get_or_init_whisper, validate_transcription_model_ready,
    TranscriptionEngine,
};
pub use parakeet_provider::ParakeetProvider;
pub use provider::{TranscriptResult, TranscriptionError, TranscriptionProvider};
pub use whisper_provider::WhisperProvider;
#[cfg(target_os = "macos")]
pub use apple_speech_provider::AppleSpeechProvider;
pub use worker::{reset_speech_detected_flag, start_transcription_task, TranscriptUpdate};
