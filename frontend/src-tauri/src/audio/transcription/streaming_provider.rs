// audio/transcription/streaming_provider.rs
//
// Streaming transcription abstraction — a sibling to the one-shot
// `TranscriptionProvider` trait for providers that hold a persistent connection
// and stream partial + final results as audio flows in (e.g. a self-hosted
// Voxtral-realtime websocket served by vLLM).
//
// The one-shot trait is request/response per VAD-gated speech segment
// (`provider.rs`). A realtime server is the opposite: one long-lived session per
// recording, continuous PCM pushed up, partial/final transcript events streamed
// back. That doesn't fit the one-shot seam, so streaming providers live behind
// this separate trait and are driven by the streaming worker rather than the
// chunk worker.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::provider::TranscriptionError;
use super::CustomTranscriptionConfig;

/// A transcript event streamed from a realtime provider.
#[derive(Debug, Clone)]
pub enum StreamTranscriptEvent {
    /// Interim hypothesis — may be revised by later partials or superseded by the
    /// final for the same segment.
    Partial { text: String },
    /// Stable transcript for a completed segment. `confidence` is `None` when the
    /// provider does not report per-segment confidence (Voxtral-realtime does not).
    Final {
        text: String,
        confidence: Option<f32>,
    },
    /// An error surfaced from inside the session.
    ///
    /// `fatal` distinguishes a transient hiccup the provider is still recovering
    /// from (a dropped socket it is reconnecting to) from a terminal one that has
    /// ended transcription for the rest of the recording. The worker surfaces the
    /// former as a warning and the latter as an actionable `transcription-error`,
    /// so a session that dies mid-recording can never fail silently.
    Error { message: String, fatal: bool },
}

/// Handle to a live streaming transcription session.
pub struct StreamSession {
    /// Push 16 kHz mono f32 audio frames to the provider. Dropping this sender
    /// (closing the channel) signals end-of-audio: the provider flushes the tail,
    /// requests the final transcript, and then the worker task exits.
    pub audio_tx: mpsc::UnboundedSender<Vec<f32>>,
    /// Hard-cancel the session — closes the socket and ends the worker task
    /// without waiting for a final flush. Prefer dropping `audio_tx` for a clean
    /// stop; use this to abort.
    pub cancel: CancellationToken,
}

/// A transcription provider that streams results over a persistent connection.
#[async_trait]
pub trait StreamingTranscriptionProvider: Send + Sync {
    /// Open a streaming session. Audio pushed to the returned
    /// [`StreamSession::audio_tx`] (16 kHz mono f32) is transcribed and results
    /// delivered on `events` until the audio channel is closed, the session is
    /// cancelled, or the remote closes the connection.
    ///
    /// `language` is a best-effort hint; providers that auto-detect (Voxtral) may
    /// ignore it.
    async fn start_session(
        &self,
        language: Option<String>,
        events: mpsc::UnboundedSender<StreamTranscriptEvent>,
    ) -> Result<StreamSession, TranscriptionError>;

    /// Verify the endpoint is reachable and correctly configured. Connects,
    /// performs the protocol handshake (which validates the model server-side),
    /// and disconnects. Used by the "Test Connection" settings command.
    async fn test_connection(&self) -> Result<(), TranscriptionError>;

    /// Provider name for logging/debugging.
    fn provider_name(&self) -> &'static str;
}

/// Build a streaming provider from persisted config, dispatching on `protocol`.
///
/// New websocket dialects (Deepgram live, etc.) plug in here without touching the
/// rest of the streaming plumbing.
pub fn build_streaming_provider(
    config: CustomTranscriptionConfig,
) -> Result<Arc<dyn StreamingTranscriptionProvider>, TranscriptionError> {
    match config.protocol.as_str() {
        "voxtral-realtime" | "" => Ok(Arc::new(
            super::voxtral_realtime::VoxtralRealtimeProvider::new(config),
        )),
        other => Err(TranscriptionError::EngineFailed(format!(
            "Unknown streaming transcription protocol '{}'. Supported: voxtral-realtime.",
            other
        ))),
    }
}
