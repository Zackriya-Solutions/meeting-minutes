// audio/transcription/mod.rs
//
// Transcription module: Provider abstraction, engine management, and worker pool.

pub mod provider;
pub mod whisper_provider;
pub mod parakeet_provider;
pub mod engine;
pub mod worker;
pub mod remote_provider;
pub mod groq_provider;
pub mod disabled_provider;

// Re-export commonly used types
pub use provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
pub use whisper_provider::WhisperProvider;
pub use parakeet_provider::ParakeetProvider;
pub use engine::{
    TranscriptionEngine,
    validate_transcription_model_ready,
    get_or_init_transcription_engine,
    get_or_init_whisper
};
pub use worker::{
    start_transcription_task,
    reset_speech_detected_flag,
    set_transcription_paused,
    is_transcription_paused,
    reset_transcription_paused_flag,
    TranscriptUpdate
};
pub use remote_provider::{RemoteProvider, RemoteProviderConfig};
pub use groq_provider::{GroqProvider, GroqConfig};
pub use disabled_provider::DisabledProvider;
