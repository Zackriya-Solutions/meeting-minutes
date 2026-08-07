// audio/transcription/mod.rs
//
// Transcription module: Provider abstraction, engine management, and worker pool.

use serde::{Deserialize, Serialize};

pub mod provider;
pub mod whisper_provider;
pub mod parakeet_provider;
pub mod engine;
pub mod worker;
pub mod streaming_provider;
pub mod voxtral_realtime;
pub mod streaming_worker;

/// Provider identifier for the custom streaming (websocket) transcription backend.
pub const CUSTOM_STREAMING_PROVIDER: &str = "customStreaming";

/// Configuration for a self-hosted realtime transcription websocket endpoint.
///
/// Stored as JSON in the `transcript_settings.customTranscriptionConfig` column and
/// used to connect to an OpenAI/Voxtral-compatible realtime ASR server (e.g. vLLM
/// serving `Voxtral-Mini-Realtime`). Mirrors the summary-side `CustomOpenAIConfig`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomTranscriptionConfig {
    /// Base URL of the websocket endpoint (e.g. "ws://localhost:8000" or a full path).
    pub endpoint: String,
    /// API key for authentication (optional if the server doesn't require it).
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
    /// Model identifier to request (e.g. "voxtral-mini-transcribe-realtime-2602").
    pub model: String,
    /// Streaming protocol dialect. Defaults to "voxtral-realtime".
    #[serde(default = "default_streaming_protocol")]
    pub protocol: String,
    /// Requested transcription delay in milliseconds (protocol-specific, optional).
    #[serde(rename = "delayMs")]
    pub delay_ms: Option<u32>,
    /// Longest stretch of audio, in seconds, to feed to a single server session
    /// before rolling over to a fresh one.
    ///
    /// A realtime ASR server holds the whole session in one bounded context; once
    /// that fills, the session stops producing transcripts for the rest of the
    /// recording. Rolling over on a schedule keeps a multi-hour meeting inside
    /// whatever the backend can actually take. `None` uses
    /// [`DEFAULT_MAX_SESSION_SECONDS`]; `Some(0)` disables rollover entirely (one
    /// session for the whole recording — the pre-0.4 behaviour).
    #[serde(rename = "maxSessionSeconds", default)]
    pub max_session_seconds: Option<u32>,
}

/// Rollover interval used when the user hasn't set one and the endpoint didn't
/// announce a context size. Conservative on purpose: it comfortably fits the
/// 8k-token context typical of a self-hosted Voxtral-Mini deployment, which in
/// practice dies somewhere past the 8-minute mark.
pub const DEFAULT_MAX_SESSION_SECONDS: u32 = 300;

impl CustomTranscriptionConfig {
    /// Seconds of audio per server session, or `None` when rollover is disabled.
    pub fn session_limit_seconds(&self) -> Option<u32> {
        match self.max_session_seconds {
            Some(0) => None,
            Some(secs) => Some(secs),
            None => Some(DEFAULT_MAX_SESSION_SECONDS),
        }
    }
}

fn default_streaming_protocol() -> String {
    "voxtral-realtime".to_string()
}

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
    TranscriptUpdate
};
pub use streaming_provider::{
    build_streaming_provider,
    StreamSession,
    StreamTranscriptEvent,
    StreamingTranscriptionProvider,
};
pub use streaming_worker::run_streaming_session;
pub use voxtral_realtime::{detect_session_limit, DetectedSessionLimit};
