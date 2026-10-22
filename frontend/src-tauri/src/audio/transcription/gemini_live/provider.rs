// audio/transcription/gemini_live/provider.rs
//
// TranscriptionProvider backed by Gemini Live, with session rotation and local fallback.
//
// - One WebSocket session is reused across VAD segments (keeps language context).
// - Sessions are rotated between segments before the ~10 minute connection cap, or as
//   soon as the server sends goAway. The new session is primed with the last
//   PRIME_SECS of the previous segment (overlapping audio) and the first result on it
//   is de-duplicated against the previously emitted text.
// - A segment that fails mid-stream is retried once on a fresh session; text from the
//   failed attempt is discarded, so nothing is emitted twice.
// - Auth failures switch to the local engine for the rest of the recording;
//   MAX_CONSECUTIVE_FAILURES transient failures pause Gemini for COOLDOWN. The failed
//   segment itself is always transcribed locally, so no audio is lost.

use super::connection::{LiveConnection, LiveError, PACE_FACTOR};
use super::dedup::trim_overlap;
use super::protocol::{SetupOptions, MODEL};
use crate::audio::transcription::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use async_trait::async_trait;
use log::{info, warn};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Rotate before the server's goAway, which arrives at 9:00 (connection closes at ~9:50).
const ROTATE_AFTER: Duration = Duration::from_secs(8 * 60 + 30);
const PRIME_SECS: f64 = 2.0;
const MIN_PRIME_SAMPLES: usize = 8000; // 0.5 s
const MAX_CONSECUTIVE_FAILURES: u32 = 3;
const COOLDOWN: Duration = Duration::from_secs(5 * 60);

/// Status notices for the UI (e.g. "switched to local transcription").
pub type StatusCallback = Arc<dyn Fn(GeminiLiveStatus) + Send + Sync>;

#[derive(Debug, Clone, serde::Serialize)]
pub struct GeminiLiveStatus {
    /// "fallback" | "resumed" | "disabled"
    pub kind: &'static str,
    pub message: String,
}

#[derive(Default)]
struct State {
    conn: Option<LiveConnection>,
    had_connection: bool,
    consecutive_failures: u32,
    paused_until: Option<Instant>,
    disabled: bool,
    last_text: String,
    tail_audio: Vec<f32>,
    dedup_next: bool,
    /// Late text from a retired session, prepended to the next result.
    carry_text: String,
}

pub struct GeminiLiveProvider {
    endpoint: String,
    api_key: String,
    setup: SetupOptions,
    fallback: Option<Arc<dyn TranscriptionProvider>>,
    on_status: StatusCallback,
    state: Mutex<State>,
}

impl GeminiLiveProvider {
    pub fn new(
        api_key: String,
        setup: SetupOptions,
        fallback: Option<Arc<dyn TranscriptionProvider>>,
        on_status: StatusCallback,
    ) -> Self {
        Self {
            endpoint: super::protocol::WS_URL.to_string(),
            api_key,
            setup,
            fallback,
            on_status,
            state: Mutex::new(State::default()),
        }
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    fn fallback_name(&self) -> &'static str {
        self.fallback.as_ref().map(|f| f.provider_name()).unwrap_or("local engine")
    }

    async fn ensure_connection(&self, st: &mut State) -> Result<(), LiveError> {
        let reason = match st.conn.as_mut() {
            None => "no session",
            Some(c) => {
                if !c.check_usable() {
                    "server ended session"
                } else if c.age() >= ROTATE_AFTER {
                    "session age limit"
                } else {
                    return Ok(());
                }
            }
        };
        if st.had_connection {
            info!("🔄 Gemini Live: opening new session ({})", reason);
        }
        // Make before break: the old session (if any) stays open until the new one is ready.
        let mut fresh = LiveConnection::connect_to(&self.endpoint, &self.api_key, &self.setup).await?;
        if st.had_connection && st.tail_audio.len() >= MIN_PRIME_SAMPLES {
            match fresh.transcribe_segment(&st.tail_audio, PACE_FACTOR).await {
                Ok(_) => st.dedup_next = true,
                Err(LiveError::Auth(m)) => return Err(LiveError::Auth(m)),
                Err(e) => warn!("Gemini Live: priming new session failed ({}); continuing without overlap", e),
            }
        }
        if let Some(mut old) = st.conn.replace(fresh) {
            let late = old.take_carry().join(" ");
            if !late.is_empty() {
                st.carry_text = late;
            }
            old.close_in_background();
        }
        st.had_connection = true;
        Ok(())
    }

    async fn transcribe_with_gemini(&self, st: &mut State, audio: &[f32]) -> Result<String, LiveError> {
        let mut last_err = None;
        for attempt in 0..2 {
            self.ensure_connection(st).await?;
            let conn = st.conn.as_mut().expect("connection ensured above");
            match conn.transcribe_segment(audio, PACE_FACTOR).await {
                Ok(text) => {
                    let text = if std::mem::take(&mut st.dedup_next) {
                        trim_overlap(&st.last_text, &text)
                    } else {
                        text
                    };
                    let text = match std::mem::take(&mut st.carry_text) {
                        late if late.is_empty() => text,
                        late if text.is_empty() => late,
                        late => format!("{} {}", late, text),
                    };
                    if !text.is_empty() {
                        st.last_text = text.clone();
                    }
                    let keep = (PRIME_SECS * 16000.0) as usize;
                    st.tail_audio = audio[audio.len().saturating_sub(keep)..].to_vec();
                    return Ok(text);
                }
                Err(LiveError::Auth(m)) => return Err(LiveError::Auth(m)),
                Err(e) => {
                    warn!("Gemini Live: segment attempt {} failed: {}", attempt + 1, e);
                    st.conn = None;
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| LiveError::Transient("unknown failure".into())))
    }

    async fn transcribe_locally(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> Result<TranscriptResult, TranscriptionError> {
        match &self.fallback {
            Some(fallback) => fallback.transcribe(audio, language).await,
            None => Err(TranscriptionError::EngineFailed(
                "Gemini Live is unavailable and no local fallback model is loaded".into(),
            )),
        }
    }
}

#[async_trait]
impl TranscriptionProvider for GeminiLiveProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> Result<TranscriptResult, TranscriptionError> {
        // Held for the whole call: the worker is serial and segments must stay in order.
        let mut st = self.state.lock().await;

        let paused = st.paused_until.map_or(false, |t| Instant::now() < t);
        if !st.disabled && !paused {
            match self.transcribe_with_gemini(&mut st, &audio).await {
                Ok(text) => {
                    st.consecutive_failures = 0;
                    if st.paused_until.take().is_some() {
                        info!("✅ Gemini Live transcription resumed");
                        (self.on_status)(GeminiLiveStatus {
                            kind: "resumed",
                            message: "Gemini Live transcription resumed.".into(),
                        });
                    }
                    return Ok(TranscriptResult { text, confidence: None, is_partial: false });
                }
                Err(LiveError::Auth(msg)) => {
                    warn!("Gemini Live disabled for this recording: {}", msg);
                    st.disabled = true;
                    st.conn = None;
                    (self.on_status)(GeminiLiveStatus {
                        kind: "disabled",
                        message: format!(
                            "Gemini Live rejected the API key ({}). Using {} for the rest of this recording.",
                            msg,
                            self.fallback_name()
                        ),
                    });
                }
                Err(e) => {
                    st.consecutive_failures += 1;
                    warn!("Gemini Live failure {}/{}: {}", st.consecutive_failures, MAX_CONSECUTIVE_FAILURES, e);
                    if st.consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                        st.consecutive_failures = 0;
                        st.paused_until = Some(Instant::now() + COOLDOWN);
                        st.conn = None;
                        (self.on_status)(GeminiLiveStatus {
                            kind: "fallback",
                            message: format!(
                                "Gemini Live keeps failing ({}). Using {} for the next {} minutes.",
                                e,
                                self.fallback_name(),
                                COOLDOWN.as_secs() / 60
                            ),
                        });
                    }
                }
            }
        }
        // While paused, Gemini is retried on the first segment after the cooldown ends.
        drop(st);
        self.transcribe_locally(audio, language).await
    }

    async fn is_model_loaded(&self) -> bool {
        true
    }

    async fn get_current_model(&self) -> Option<String> {
        Some(MODEL.to_string())
    }

    fn provider_name(&self) -> &'static str {
        "Gemini Live"
    }
}
