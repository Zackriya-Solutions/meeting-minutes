// audio/deepgram.rs
//
// First-class Deepgram transcription provider.
//
// Deepgram is a streaming-first cloud ASR: live recording uses the realtime
// WebSocket API (low-latency interim + final results), while imported files and
// retranscription use the prerecorded REST API. This does NOT map onto the
// batch `TranscriptionProvider` trait (chunk -> Vec<f32> -> text), so Deepgram
// is wired in as its own streaming path and the trait-based engine is bypassed
// for this provider.
//
// Wire contract (verified live against api.deepgram.com, nova-3):
//   - Auth header is `Authorization: Token <key>` (NOT `Bearer`).
//   - Prerecorded: a single synchronous POST of the audio bytes; the response
//     carries `results.utterances[]` (speaker-segmented turns) and
//     `results.channels[0].alternatives[0].{transcript,words}`. No polling.
//   - Realtime: params are passed as WS query string (not a config frame). Each
//     `Results` message carries `is_final`/`speech_final` and a smart-formatted
//     `channel.alternatives[0].transcript` plus word-level timing.

use anyhow::{anyhow, Context, Result};
use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use serde::Deserialize;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, Message},
};

use crate::api::TranscriptSegment;
use crate::database::repositories::setting::SettingsRepository;
use crate::state::AppState;

use super::recording_state::AudioChunk;
use super::transcription::TranscriptUpdate;

pub const PROVIDER: &str = "deepgram";
const LISTEN_REST_URL: &str = "https://api.deepgram.com/v1/listen";
const LISTEN_WS_URL: &str = "wss://api.deepgram.com/v1/listen";
// Flux uses a separate v2 realtime endpoint with turn-based events.
const FLUX_WS_URL: &str = "wss://api.deepgram.com/v2/listen";
const PCM_SAMPLE_RATE: u32 = 16_000;
/// Deepgram closes an idle realtime socket after ~10s; send KeepAlive well under that.
const KEEPALIVE_SECS: u64 = 5;

static REALTIME_SEQUENCE_COUNTER: AtomicU64 = AtomicU64::new(0);
static REALTIME_SPEECH_DETECTED_EMITTED: AtomicBool = AtomicBool::new(false);

// ============================================================================
// OPTIONS
// ============================================================================

/// Provider-specific knobs surfaced in Settings (model + keyterm prompting +
/// diarization). These are what make Deepgram worth wiring first-class rather
/// than flattening to a generic "text out" contract.
#[derive(Debug, Clone)]
pub struct DeepgramOptions {
    pub model: String,
    pub keyterms: Vec<String>,
    pub diarize: bool,
    /// Opt out of Deepgram's Model Improvement Program (sends `mip_opt_out=true`).
    /// Defaults to true: Meetily is privacy-first, so audio is not retained for
    /// model training unless the user explicitly opts back in.
    pub mip_opt_out: bool,
    /// Deepgram-specific language mode (`multi`, `detect`, or an ISO code). Kept
    /// separate from the shared global language pref so Deepgram-only values never
    /// leak into the Whisper/Parakeet path. None -> fall back to the caller's hint.
    pub language: Option<String>,
}

impl Default for DeepgramOptions {
    fn default() -> Self {
        Self {
            model: crate::config::DEFAULT_DEEPGRAM_REALTIME_MODEL.to_string(),
            keyterms: Vec::new(),
            diarize: true,
            mip_opt_out: true,
            language: None,
        }
    }
}

// ============================================================================
// RESPONSE TYPES
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct DeepgramWord {
    #[serde(default)]
    pub word: String,
    #[serde(default)]
    pub punctuated_word: Option<String>,
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub end: f64,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub speaker: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct Alternative {
    #[serde(default)]
    transcript: String,
    #[serde(default)]
    confidence: Option<f32>,
    #[serde(default)]
    words: Vec<DeepgramWord>,
}

// Deepgram splits utterances by speaker when diarization is on, so each utterance is
// already a single-speaker turn. meetily's transcript model has no diarization
// speaker field (its `speaker` column is audio-source: mic/system), so the
// numeric speaker id is intentionally not carried — diarization manifests as
// speaker-split segmentation, consistent with the existing Whisper/Parakeet behavior.
#[derive(Debug, Clone, Deserialize)]
pub struct Utterance {
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub end: f64,
    #[serde(default)]
    pub transcript: String,
}

#[derive(Debug, Deserialize)]
struct Channel {
    #[serde(default)]
    alternatives: Vec<Alternative>,
}

#[derive(Debug, Deserialize)]
struct PrerecordedResults {
    #[serde(default)]
    channels: Vec<Channel>,
    #[serde(default)]
    utterances: Vec<Utterance>,
}

#[derive(Debug, Deserialize)]
struct PrerecordedMetadata {
    #[serde(default)]
    duration: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct PrerecordedResponse {
    results: PrerecordedResults,
    #[serde(default)]
    metadata: Option<PrerecordedMetadata>,
}

/// Result of a prerecorded transcription, normalized for segment mapping.
#[derive(Debug, Clone)]
pub struct DeepgramTranscript {
    pub text: String,
    pub utterances: Vec<Utterance>,
    pub words: Vec<DeepgramWord>,
    pub duration_seconds: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct RealtimeChannel {
    #[serde(default)]
    alternatives: Vec<Alternative>,
}

#[derive(Debug, Deserialize)]
struct RealtimeMessage {
    #[serde(default, rename = "type")]
    msg_type: Option<String>,
    #[serde(default)]
    channel: Option<RealtimeChannel>,
    #[serde(default)]
    is_final: bool,
    #[serde(default)]
    start: f64,
    #[serde(default)]
    duration: f64,
    // A `type: "Error"` message carries these; both are optional in the schema.
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

/// Flux (`/v2/listen`) turn-based message. The finalized turn transcript arrives
/// on `event == "EndOfTurn"` (natively punctuated); Update/StartOfTurn/Eager/
/// TurnResumed are interim. `audio_window_start/end` are stream-relative seconds.
#[derive(Debug, Deserialize)]
struct FluxTurnInfo {
    #[serde(default, rename = "type")]
    msg_type: Option<String>,
    #[serde(default)]
    event: Option<String>,
    #[serde(default)]
    transcript: String,
    #[serde(default)]
    audio_window_start: f64,
    #[serde(default)]
    audio_window_end: f64,
    #[serde(default)]
    words: Vec<DeepgramWord>,
    // Present on a `type: "Error"` control message.
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

/// A finalized segment reduced from either protocol, in stream-relative ms.
struct FinalSeg {
    transcript: String,
    start_ms: f64,
    end_ms: f64,
    confidence: f32,
}

/// Reduce a nova `Results` message to a finalized segment (None for interim/other).
fn nova_result_to_final(m: RealtimeMessage) -> Option<FinalSeg> {
    if m.msg_type.as_deref() != Some("Results") || !m.is_final {
        return None;
    }
    let alternative = m.channel.and_then(|c| c.alternatives.into_iter().next())?;
    let transcript = alternative.transcript.trim();
    if transcript.is_empty() {
        return None;
    }
    let confidence = alternative
        .confidence
        .unwrap_or_else(|| average_confidence(alternative.words.iter().filter_map(|w| w.confidence)));
    Some(FinalSeg {
        transcript: transcript.to_string(),
        start_ms: m.start * 1000.0,
        end_ms: (m.start + m.duration) * 1000.0,
        confidence,
    })
}

/// Reduce a Flux `TurnInfo` to a finalized segment (only on `EndOfTurn`).
fn flux_turn_to_final(m: &FluxTurnInfo) -> Option<FinalSeg> {
    if m.event.as_deref() != Some("EndOfTurn") {
        return None;
    }
    let transcript = m.transcript.trim();
    if transcript.is_empty() {
        return None;
    }
    Some(FinalSeg {
        transcript: transcript.to_string(),
        start_ms: m.audio_window_start * 1000.0,
        end_ms: m.audio_window_end * 1000.0,
        confidence: average_confidence(m.words.iter().filter_map(|w| w.confidence)),
    })
}

/// Maps realtime-stream time onto original recording time. The pipeline feeds a
/// continuous mixed stream while carrying each chunk's recording-relative
/// timestamp, so realtime word times (stream-relative) are remapped back.
#[derive(Debug, Clone)]
pub struct AudioTimeMapEntry {
    pub stream_start_ms: f64,
    pub stream_end_ms: f64,
    pub original_start_ms: f64,
    pub original_end_ms: f64,
}

// ============================================================================
// CLIENT (prerecorded REST)
// ============================================================================

#[derive(Debug, Clone)]
pub struct DeepgramClient {
    api_key: String,
    http: reqwest::Client,
    options: DeepgramOptions,
}

impl DeepgramClient {
    pub fn new(api_key: String, options: DeepgramOptions) -> Self {
        Self {
            api_key,
            http: reqwest::Client::new(),
            options,
        }
    }

    /// Transcribe a local file via the prerecorded REST API. This is a single
    /// synchronous request (no upload -> create -> poll -> fetch cycle),
    /// so progress is coarse: submit, then parse.
    pub async fn transcribe_file_with_progress<F, C>(
        &self,
        path: &Path,
        language: Option<String>,
        _client_reference_id: &str,
        mut progress: F,
        mut should_cancel: C,
    ) -> Result<DeepgramTranscript>
    where
        F: FnMut(u32, &str),
        C: FnMut() -> bool,
    {
        ensure_not_cancelled(&mut should_cancel)?;
        progress(10, "Reading audio file...");
        let bytes = tokio::fs::read(path)
            .await
            .with_context(|| format!("Failed to read audio file: {}", path.display()))?;

        ensure_not_cancelled(&mut should_cancel)?;
        progress(25, "Uploading audio to Deepgram...");

        let query = self.prerecorded_query(language);
        let content_type = mime_for_path(path);
        let response = self
            .http
            .post(LISTEN_REST_URL)
            .query(&query)
            .header("Authorization", format!("Token {}", self.api_key))
            .header("Content-Type", content_type)
            .body(bytes)
            .send()
            .await
            .context("Failed to submit audio to Deepgram")?;

        let response = ensure_success(response, "prerecorded transcription").await?;

        ensure_not_cancelled(&mut should_cancel)?;
        progress(85, "Parsing Deepgram transcript...");
        let payload: PrerecordedResponse = response
            .json()
            .await
            .context("Failed to parse Deepgram prerecorded response")?;

        let (text, words) = payload
            .results
            .channels
            .into_iter()
            .next()
            .and_then(|channel| channel.alternatives.into_iter().next())
            .map(|alt| (alt.transcript, alt.words))
            .unwrap_or_default();

        Ok(DeepgramTranscript {
            text,
            utterances: payload.results.utterances,
            words,
            duration_seconds: payload.metadata.and_then(|m| m.duration),
        })
    }

    /// Query parameters for the prerecorded endpoint. `smart_format` and
    /// `punctuate` give readable transcripts; `utterances` yields speaker turns.
    fn prerecorded_query(&self, language: Option<String>) -> Vec<(String, String)> {
        let model = self.prerecorded_model();
        let mut query = vec![
            ("model".to_string(), model.to_string()),
            ("smart_format".to_string(), "true".to_string()),
            ("punctuate".to_string(), "true".to_string()),
            ("utterances".to_string(), "true".to_string()),
            ("paragraphs".to_string(), "true".to_string()),
        ];
        if self.options.diarize {
            // Deepgram's current diarizer. `diarize_model` is mutually exclusive with
            // `diarize`/`diarize_version` (sending both -> HTTP 400), so emit ONLY
            // diarize_model=latest here, never diarize=true. Verified live against
            // api.deepgram.com (both prerecorded and streaming return 200/101).
            query.push(("diarize_model".to_string(), "latest".to_string()));
        }
        if self.options.mip_opt_out {
            query.push(("mip_opt_out".to_string(), "true".to_string()));
        }
        // Deepgram-specific language wins; fall back to the caller's hint if unset.
        let lang = self.options.language.as_deref().or(language.as_deref());
        apply_language(&mut query, lang, model);
        apply_keyterms(&mut query, &self.options.keyterms);
        query
    }

    /// The model to use for prerecorded (file) transcription. Flux is a
    /// realtime-only API with no `/v1/listen` prerecorded support, so imported
    /// audio / retranscription falls back to the default nova async model.
    fn prerecorded_model(&self) -> &str {
        if is_flux_model(&self.options.model) {
            crate::config::DEFAULT_DEEPGRAM_ASYNC_MODEL
        } else {
            &self.options.model
        }
    }
}

// ============================================================================
// CONFIG READERS
// ============================================================================

pub async fn configured_api_key<R: Runtime>(app: &AppHandle<R>) -> Result<String> {
    let app_state = app
        .try_state::<AppState>()
        .ok_or_else(|| anyhow!("App state not available"))?;

    let stored_key =
        SettingsRepository::get_transcript_api_key(app_state.db_manager.pool(), PROVIDER)
            .await
            .context("Failed to read Deepgram API key from settings")?;

    let key = stored_key
        .filter(|key| !key.trim().is_empty())
        .or_else(|| std::env::var("DEEPGRAM_API_KEY").ok())
        .unwrap_or_default();

    if key.trim().is_empty() {
        return Err(anyhow!(
            "Deepgram API key is not configured. Add it in Transcription settings or set DEEPGRAM_API_KEY."
        ));
    }

    Ok(key)
}

pub async fn validate_configured_api_key<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    configured_api_key(app)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Reads the persisted Deepgram options: model (from transcript config) plus
/// keyterms/diarize (from the dedicated Deepgram settings columns).
pub async fn configured_options<R: Runtime>(app: &AppHandle<R>) -> Result<DeepgramOptions> {
    let app_state = app
        .try_state::<AppState>()
        .ok_or_else(|| anyhow!("App state not available"))?;
    let pool = app_state.db_manager.pool();

    let mut options = DeepgramOptions::default();

    if let Ok(Some(config)) = SettingsRepository::get_transcript_config(pool).await {
        if config.provider == PROVIDER && !config.model.trim().is_empty() {
            options.model = config.model;
        }
    }

    let row = SettingsRepository::get_deepgram_options(pool)
        .await
        .unwrap_or_default();
    options.keyterms = parse_keyterms(row.keyterm.as_deref());
    if let Some(diarize) = row.diarize {
        options.diarize = diarize != 0;
    }
    if let Some(mip_opt_out) = row.mip_opt_out {
        options.mip_opt_out = mip_opt_out != 0;
    }
    options.language = row.language.filter(|l| !l.trim().is_empty());

    Ok(options)
}

pub async fn client_from_config<R: Runtime>(app: &AppHandle<R>) -> Result<DeepgramClient> {
    let api_key = configured_api_key(app).await?;
    let options = configured_options(app).await?;
    Ok(DeepgramClient::new(api_key, options))
}

// ============================================================================
// SEGMENT MAPPING (prerecorded)
// ============================================================================

/// Convert a prerecorded transcript to segments. Prefers `utterances` (already
/// speaker-segmented turns); falls back to word grouping, then to a single
/// segment covering the whole file.
pub fn transcript_to_segments(
    transcript: &DeepgramTranscript,
    fallback_duration_seconds: f64,
) -> Vec<TranscriptSegment> {
    if !transcript.utterances.is_empty() {
        let tuples = utterances_to_tuples(&transcript.utterances);
        let segments = super::common::create_transcript_segments(&tuples);
        if !segments.is_empty() {
            return segments;
        }
    }

    let mut segments = super::common::create_transcript_segments(&words_to_tuples(&transcript.words));
    if segments.is_empty() && !transcript.text.trim().is_empty() {
        // Prefer Deepgram's own reported duration; fall back to the caller's.
        let total_duration = transcript.duration_seconds.unwrap_or(fallback_duration_seconds);
        segments.push(TranscriptSegment {
            id: format!("transcript-{}", uuid::Uuid::new_v4()),
            text: transcript.text.trim().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            audio_start_time: Some(0.0),
            audio_end_time: Some(total_duration),
            duration: Some(total_duration),
        });
    }
    segments
}

fn utterances_to_tuples(utterances: &[Utterance]) -> Vec<(String, f64, f64)> {
    let mut tuples = Vec::new();
    for utterance in utterances {
        push_tuple(
            &mut tuples,
            utterance.transcript.clone(),
            utterance.start * 1000.0,
            utterance.end * 1000.0,
        );
    }
    tuples
}

/// Group word-level output into segments on speaker change, long pause, or a
/// long sentence-terminated run, so transcript rows stay a readable size.
fn words_to_tuples(words: &[DeepgramWord]) -> Vec<(String, f64, f64)> {
    let mut segments = Vec::new();
    let mut current_text = String::new();
    let mut current_start_ms = 0.0;
    let mut current_end_ms = 0.0;
    let mut current_speaker: Option<i64> = None;

    for word in words {
        let display = word.punctuated_word.as_deref().unwrap_or(&word.word);
        if display.is_empty() {
            continue;
        }

        let word_start_ms = word.start * 1000.0;
        let word_end_ms = (word.end * 1000.0).max(word_start_ms);
        let speaker_changed = current_speaker.is_some() && word.speaker != current_speaker;
        let long_gap = !current_text.is_empty() && word_start_ms - current_end_ms > 1500.0;
        let long_segment =
            word_start_ms - current_start_ms > 30_000.0 && ends_sentence(&current_text);

        if !current_text.is_empty() && (speaker_changed || long_gap || long_segment) {
            push_tuple(
                &mut segments,
                std::mem::take(&mut current_text),
                current_start_ms,
                current_end_ms,
            );
            current_speaker = None;
        }

        if current_text.is_empty() {
            current_start_ms = word_start_ms;
            current_speaker = word.speaker;
        } else {
            current_text.push(' ');
        }

        current_text.push_str(display);
        current_end_ms = word_end_ms;
    }

    push_tuple(&mut segments, current_text, current_start_ms, current_end_ms);
    segments
}

// ============================================================================
// REALTIME (WebSocket)
// ============================================================================

pub async fn run_realtime_transcription<R: Runtime>(
    app: AppHandle<R>,
    transcription_receiver: mpsc::UnboundedReceiver<AudioChunk>,
) -> Result<(), String> {
    REALTIME_SPEECH_DETECTED_EMITTED.store(false, Ordering::SeqCst);

    let api_key = configured_api_key(&app).await.map_err(|e| e.to_string())?;
    let options = configured_options(&app).await.map_err(|e| e.to_string())?;
    let language = crate::get_language_preference_internal();

    // Flux is a distinct realtime API: /v2/listen with turn-based `TurnInfo` events
    // and its own model-selected language handling. nova uses /v1/listen `Results`.
    let flux = is_flux_model(&options.model);
    let url = if flux {
        build_flux_url(&options, language)
    } else {
        build_realtime_url(&options, language)
    };
    let mut request = url
        .into_client_request()
        .map_err(|e| format!("Failed to build Deepgram WebSocket request: {}", e))?;
    // Deepgram uses the "Token" scheme, not "Bearer".
    let auth_header = HeaderValue::from_str(&format!("Token {}", api_key))
        .map_err(|e| format!("Failed to build Deepgram auth header: {}", e))?;
    request.headers_mut().insert("Authorization", auth_header);

    info!("Connecting to Deepgram realtime transcription ({})", options.model);
    let writer_flux = flux;
    let reader_flux = flux;
    let (ws_stream, _) = connect_async(request)
        .await
        .map_err(|e| format!("Failed to connect to Deepgram realtime API: {}", e))?;
    let (mut write, mut read) = ws_stream.split();

    let time_map = Arc::new(Mutex::new(Vec::<AudioTimeMapEntry>::new()));
    let stream_cursor_ms = Arc::new(Mutex::new(0.0f64));
    let shutdown = Arc::new(AtomicBool::new(false));

    let writer_time_map = time_map.clone();
    let writer_cursor = stream_cursor_ms.clone();
    let writer_shutdown = shutdown.clone();
    let writer_handle = tokio::spawn(async move {
        let mut receiver = transcription_receiver;
        let mut keepalive = tokio::time::interval(Duration::from_secs(KEEPALIVE_SECS));
        let mut shutdown_interval = tokio::time::interval(Duration::from_millis(250));

        loop {
            tokio::select! {
                _ = shutdown_interval.tick() => {
                    if writer_shutdown.load(Ordering::SeqCst) {
                        break;
                    }
                }
                _ = keepalive.tick() => {
                    // nova closes an idle socket after ~10s; Flux manages turns itself
                    // and does not accept the KeepAlive control frame.
                    if !writer_flux {
                        let msg = serde_json::json!({ "type": "KeepAlive" }).to_string();
                        if let Err(e) = write.send(Message::Text(msg)).await {
                            return Err(anyhow!("Failed to send Deepgram KeepAlive: {}", e));
                        }
                    }
                }
                maybe_chunk = receiver.recv() => {
                    let Some(chunk) = maybe_chunk else {
                        break;
                    };

                    if chunk.data.is_empty() {
                        continue;
                    }

                    let original_start_ms = chunk.timestamp * 1000.0;
                    let original_duration_ms =
                        chunk.data.len() as f64 / chunk.sample_rate as f64 * 1000.0;
                    let original_end_ms = original_start_ms + original_duration_ms;

                    let pcm_samples = if chunk.sample_rate != PCM_SAMPLE_RATE {
                        super::audio_processing::resample_audio(&chunk.data, chunk.sample_rate, PCM_SAMPLE_RATE)
                    } else {
                        chunk.data
                    };

                    if pcm_samples.is_empty() {
                        continue;
                    }

                    let duration_ms = pcm_samples.len() as f64 / PCM_SAMPLE_RATE as f64 * 1000.0;
                    {
                        let mut cursor = writer_cursor.lock().await;
                        let entry = AudioTimeMapEntry {
                            stream_start_ms: *cursor,
                            stream_end_ms: *cursor + duration_ms,
                            original_start_ms,
                            original_end_ms,
                        };
                        *cursor += duration_ms;
                        writer_time_map.lock().await.push(entry);
                    }

                    let bytes = f32_to_pcm16le(&pcm_samples);
                    if let Err(e) = write.send(Message::Binary(bytes)).await {
                        return Err(anyhow!("Failed to send audio to Deepgram realtime API: {}", e));
                    }
                }
            }
        }

        // nova: flush pending audio and finalize via CloseStream. Flux: just drop
        // the write half (closing the socket ends the turn); it has no CloseStream.
        if !writer_flux {
            let close = serde_json::json!({ "type": "CloseStream" }).to_string();
            let _ = write.send(Message::Text(close)).await;
        }
        Ok::<(), anyhow::Error>(())
    });

    let reader_time_map = time_map.clone();
    let reader_shutdown = shutdown.clone();
    let reader_app = app.clone();
    let reader_handle = tokio::spawn(async move {
        while let Some(message) = read.next().await {
            let message = match message {
                Ok(message) => message,
                Err(e) => {
                    reader_shutdown.store(true, Ordering::SeqCst);
                    return Err(anyhow!("Deepgram realtime WebSocket error: {}", e));
                }
            };

            match message {
                Message::Text(text) => {
                    // Parse per protocol and reduce to an optional finalized segment
                    // (interim/Metadata/StartOfTurn/Update/etc. produce None). Both
                    // report a `type:"Error"` control message on failure.
                    let final_seg = if reader_flux {
                        match serde_json::from_str::<FluxTurnInfo>(&text) {
                            Ok(m) => {
                                if m.msg_type.as_deref() == Some("Error") {
                                    let err = m
                                        .description
                                        .or(m.message)
                                        .unwrap_or_else(|| "unknown error".to_string());
                                    reader_shutdown.store(true, Ordering::SeqCst);
                                    return Err(anyhow!("Deepgram Flux realtime error: {}", err));
                                }
                                flux_turn_to_final(&m)
                            }
                            Err(e) => {
                                warn!("Failed to parse Deepgram Flux response: {} ({})", e, text);
                                None
                            }
                        }
                    } else {
                        match serde_json::from_str::<RealtimeMessage>(&text) {
                            Ok(m) => {
                                if m.msg_type.as_deref() == Some("Error") {
                                    let err = m
                                        .description
                                        .or(m.message)
                                        .unwrap_or_else(|| "unknown error".to_string());
                                    reader_shutdown.store(true, Ordering::SeqCst);
                                    return Err(anyhow!("Deepgram realtime error: {}", err));
                                }
                                nova_result_to_final(m)
                            }
                            Err(e) => {
                                warn!("Failed to parse Deepgram realtime response: {} ({})", e, text);
                                None
                            }
                        }
                    };

                    if let Some(seg) = final_seg {
                        let entries = reader_time_map.lock().await.clone();
                        let start_ms = map_stream_ms_to_original(&entries, seg.start_ms);
                        let end_ms =
                            map_stream_ms_to_original(&entries, seg.end_ms).max(start_ms);
                        emit_speech_detected(&reader_app);
                        emit_transcript_update(
                            &reader_app,
                            seg.transcript,
                            start_ms,
                            end_ms,
                            seg.confidence,
                            false,
                        );
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }

        reader_shutdown.store(true, Ordering::SeqCst);
        Ok::<(), anyhow::Error>(())
    });

    let writer_result = writer_handle
        .await
        .map_err(|e| format!("Deepgram realtime writer task failed: {}", e))?;
    // On any error we return Err; the transcription worker owns the single
    // `transcription-error` emit, so we do not emit here (avoids double events).
    if let Err(e) = writer_result {
        shutdown.store(true, Ordering::SeqCst);
        return Err(e.to_string());
    }

    let mut reader_handle = reader_handle;
    match tokio::time::timeout(Duration::from_secs(30), &mut reader_handle).await {
        Ok(join_result) => {
            let reader_result =
                join_result.map_err(|e| format!("Deepgram realtime reader task failed: {}", e))?;
            if let Err(e) = reader_result {
                return Err(e.to_string());
            }
        }
        Err(_) => {
            reader_handle.abort();
            warn!("Timed out waiting for Deepgram realtime final response");
        }
    }

    info!("Deepgram realtime transcription task completed");
    Ok(())
}

pub fn map_stream_ms_to_original(entries: &[AudioTimeMapEntry], stream_ms: f64) -> f64 {
    if entries.is_empty() {
        return stream_ms;
    }

    if let Some(entry) = entries
        .iter()
        .find(|entry| stream_ms >= entry.stream_start_ms && stream_ms <= entry.stream_end_ms)
    {
        let stream_duration = (entry.stream_end_ms - entry.stream_start_ms).max(1.0);
        let original_duration = (entry.original_end_ms - entry.original_start_ms).max(0.0);
        let offset_ratio = ((stream_ms - entry.stream_start_ms) / stream_duration).clamp(0.0, 1.0);
        return entry.original_start_ms + original_duration * offset_ratio;
    }

    if stream_ms < entries[0].stream_start_ms {
        return entries[0].original_start_ms;
    }

    let last = entries.last().expect("entries is not empty");
    last.original_end_ms + (stream_ms - last.stream_end_ms).max(0.0)
}

/// True for Flux models (`flux-general-en`, `flux-general-multi`), which use the
/// separate v2 turn-based realtime API rather than the nova `/v1/listen` path.
fn is_flux_model(model: &str) -> bool {
    model.starts_with("flux")
}

fn build_realtime_url(options: &DeepgramOptions, language: Option<String>) -> String {
    let mut query: Vec<(String, String)> = vec![
        ("encoding".to_string(), "linear16".to_string()),
        ("sample_rate".to_string(), PCM_SAMPLE_RATE.to_string()),
        ("channels".to_string(), "1".to_string()),
        ("model".to_string(), options.model.clone()),
        ("smart_format".to_string(), "true".to_string()),
        ("punctuate".to_string(), "true".to_string()),
        // Finals only: the transcript UI buffers by sequence id with no
        // partial-replace path, so interim results would duplicate segments.
        ("interim_results".to_string(), "false".to_string()),
        ("endpointing".to_string(), "300".to_string()),
    ];
    if options.diarize {
        // diarize_model=latest replaces diarize=true (400 if both sent). See
        // prerecorded_query; verified live that streaming accepts it (101 handshake).
        query.push(("diarize_model".to_string(), "latest".to_string()));
    }
    if options.mip_opt_out {
        query.push(("mip_opt_out".to_string(), "true".to_string()));
    }
    // Deepgram-specific language wins; fall back to the caller's hint if unset.
    let lang = options.language.as_deref().or(language.as_deref());
    apply_language(&mut query, lang, &options.model);
    apply_keyterms(&mut query, &options.keyterms);
    encode_ws_url(LISTEN_WS_URL, &query)
}

/// Build the Flux v2 realtime URL. Flux does its own turn detection and
/// formatting, so it takes none of the nova knobs (smart_format/punctuate/
/// diarize/endpointing/language). The multilingual model accepts `language_hint`;
/// language is otherwise selected by the model name, not a `language` param.
fn build_flux_url(options: &DeepgramOptions, language: Option<String>) -> String {
    let mut query: Vec<(String, String)> = vec![
        ("model".to_string(), options.model.clone()),
        ("encoding".to_string(), "linear16".to_string()),
        ("sample_rate".to_string(), PCM_SAMPLE_RATE.to_string()),
    ];
    if options.model.ends_with("multi") {
        if let Some(hint) = language.as_deref().map(str::trim).filter(|l| {
            !l.is_empty() && *l != "auto" && *l != "auto-translate" && *l != "detect"
        }) {
            query.push(("language_hint".to_string(), hint.to_string()));
        }
    }
    apply_keyterms(&mut query, &options.keyterms);
    encode_ws_url(FLUX_WS_URL, &query)
}

fn encode_ws_url(base: &str, query: &[(String, String)]) -> String {
    let encoded = query
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{}?{}", base, encoded)
}

// ============================================================================
// EMITTERS (shared with the local-engine worker event contract)
// ============================================================================

fn emit_speech_detected<R: Runtime>(app: &AppHandle<R>) {
    if REALTIME_SPEECH_DETECTED_EMITTED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        let _ = app.emit(
            "speech-detected",
            serde_json::json!({ "message": "Speech activity detected" }),
        );
    }
}

fn emit_transcript_update<R: Runtime>(
    app: &AppHandle<R>,
    text: String,
    start_ms: f64,
    end_ms: f64,
    confidence: f32,
    is_partial: bool,
) {
    let sequence_id = REALTIME_SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst);
    let audio_start_time = start_ms / 1000.0;
    let audio_end_time = end_ms.max(start_ms) / 1000.0;
    let duration = (audio_end_time - audio_start_time).max(0.0);

    let update = TranscriptUpdate {
        text,
        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        source: "Audio".to_string(),
        sequence_id,
        chunk_start_time: audio_start_time,
        is_partial,
        confidence,
        audio_start_time,
        audio_end_time,
        duration,
    };

    if let Err(e) = app.emit("transcript-update", &update) {
        error!("Failed to emit Deepgram transcript update: {}", e);
    }
}

// ============================================================================
// HELPERS
// ============================================================================

/// Map the stored language preference to Deepgram query params. Three distinct modes:
///   `detect`    -> `detect_language=true`  (auto-detect a single language)
///   `multi`     -> `language=multi`        (nova-3 code-switching across 10 languages)
///   ISO code    -> `language=<code>`       (explicit selection, e.g. `es`)
/// When unset/auto, nova-3 defaults to `multi` (its code-switching default) and other
/// models fall back to Deepgram's own default. Previously `detect` was silently dropped
/// and auto collapsed to `multi`, so single-language detection was unreachable.
fn apply_language(query: &mut Vec<(String, String)>, language: Option<&str>, model: &str) {
    match language.map(str::trim).unwrap_or("") {
        "detect" => query.push(("detect_language".to_string(), "true".to_string())),
        "multi" => query.push(("language".to_string(), "multi".to_string())),
        "" | "auto" | "auto-translate" => {
            if model.starts_with("nova-3") {
                query.push(("language".to_string(), "multi".to_string()));
            }
        }
        lang => query.push(("language".to_string(), lang.to_string())),
    }
}

/// keyterm prompting. The only supported models (nova-3 and Flux) both use the
/// `keyterm` parameter, so every term routes there. (nova-2's `keywords` param was
/// dropped along with nova-2 model support.)
fn apply_keyterms(query: &mut Vec<(String, String)>, keyterms: &[String]) {
    if keyterms.is_empty() {
        return;
    }
    for term in keyterms {
        let term = term.trim();
        if !term.is_empty() {
            query.push(("keyterm".to_string(), term.to_string()));
        }
    }
}

/// Split a stored keyterm blob (newline- or comma-separated) into terms.
pub fn parse_keyterms(raw: Option<&str>) -> Vec<String> {
    raw.map(|raw| {
        raw.split(['\n', ','])
            .map(str::trim)
            .filter(|term| !term.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

fn mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("wav") => "audio/wav",
        Some("mp3") => "audio/mpeg",
        Some("m4a" | "mp4") => "audio/mp4",
        Some("flac") => "audio/flac",
        Some("ogg" | "opus") => "audio/ogg",
        Some("webm") => "audio/webm",
        _ => "application/octet-stream",
    }
}

fn f32_to_pcm16le(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let pcm = (clamped * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&pcm.to_le_bytes());
    }
    bytes
}

async fn ensure_success(response: reqwest::Response, action: &str) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(anyhow!(
        "Deepgram {} failed (HTTP {}): {}",
        action,
        status,
        truncate_for_error(&body)
    ))
}

fn ensure_not_cancelled<C>(should_cancel: &mut C) -> Result<()>
where
    C: FnMut() -> bool,
{
    if should_cancel() {
        Err(anyhow!("Operation cancelled"))
    } else {
        Ok(())
    }
}

fn push_tuple(segments: &mut Vec<(String, f64, f64)>, text: String, start_ms: f64, end_ms: f64) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    segments.push((trimmed.to_string(), start_ms, end_ms.max(start_ms)));
}

fn average_confidence(values: impl Iterator<Item = f32>) -> f32 {
    let mut sum = 0.0;
    let mut count = 0usize;
    for value in values {
        sum += value;
        count += 1;
    }
    if count == 0 {
        0.85
    } else {
        sum / count as f32
    }
}

fn ends_sentence(text: &str) -> bool {
    text.trim_end()
        .chars()
        .last()
        .map(|c| matches!(c, '.' | '!' | '?' | '\n'))
        .unwrap_or(false)
}

fn truncate_for_error(text: &str) -> String {
    const MAX_LEN: usize = 500;
    if text.len() <= MAX_LEN {
        return text.to_string();
    }
    let mut end = MAX_LEN;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

/// Minimal percent-encoding for query values (keyterms may contain spaces).
fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(w: &str, start: f64, end: f64, speaker: Option<i64>) -> DeepgramWord {
        DeepgramWord {
            word: w.to_string(),
            punctuated_word: Some(w.to_string()),
            start,
            end,
            confidence: Some(0.9),
            speaker,
        }
    }

    #[test]
    fn utterances_map_to_one_segment_each() {
        let transcript = DeepgramTranscript {
            text: "ignored".to_string(),
            utterances: vec![
                Utterance {
                    start: 0.0,
                    end: 1.5,
                    transcript: "Hello there.".to_string(),
                },
                Utterance {
                    start: 2.0,
                    end: 3.0,
                    transcript: "General Kenobi.".to_string(),
                },
            ],
            words: vec![],
            duration_seconds: Some(3.0),
        };
        let segments = transcript_to_segments(&transcript, 3.0);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Hello there.");
        assert_eq!(segments[1].text, "General Kenobi.");
        assert_eq!(segments[0].audio_start_time, Some(0.0));
        assert_eq!(segments[1].audio_start_time, Some(2.0));
    }

    #[test]
    fn words_split_on_speaker_change() {
        let words = vec![
            word("Hello", 0.0, 0.4, Some(0)),
            word("world.", 0.4, 0.9, Some(0)),
            word("Reply", 1.0, 1.4, Some(1)),
        ];
        let tuples = words_to_tuples(&words);
        assert_eq!(tuples.len(), 2);
        assert_eq!(tuples[0].0, "Hello world.");
        assert_eq!(tuples[1].0, "Reply");
    }

    #[test]
    fn falls_back_to_single_segment_when_only_text() {
        let transcript = DeepgramTranscript {
            text: "Whole thing.".to_string(),
            utterances: vec![],
            words: vec![],
            duration_seconds: Some(5.0),
        };
        let segments = transcript_to_segments(&transcript, 5.0);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Whole thing.");
        assert_eq!(segments[0].audio_end_time, Some(5.0));
    }

    #[test]
    fn maps_stream_time_to_original_chunk_time() {
        let entries = vec![
            AudioTimeMapEntry {
                stream_start_ms: 0.0,
                stream_end_ms: 1000.0,
                original_start_ms: 5000.0,
                original_end_ms: 6000.0,
            },
            AudioTimeMapEntry {
                stream_start_ms: 1000.0,
                stream_end_ms: 2000.0,
                original_start_ms: 9000.0,
                original_end_ms: 10_000.0,
            },
        ];
        assert_eq!(map_stream_ms_to_original(&entries, 500.0), 5500.0);
        assert_eq!(map_stream_ms_to_original(&entries, 1500.0), 9500.0);
    }

    #[test]
    fn nova3_keyterms_use_keyterm_param() {
        let mut query = vec![("model".to_string(), "nova-3".to_string())];
        apply_keyterms(&mut query, &["Meetily".to_string(), "Deepgram".to_string()]);
        let keyterms: Vec<_> = query.iter().filter(|(k, _)| k == "keyterm").collect();
        assert_eq!(keyterms.len(), 2);
    }

    #[test]
    fn language_defaults_to_multi_for_nova3_only() {
        let mut q1 = vec![];
        apply_language(&mut q1, None, "nova-3");
        assert_eq!(q1, vec![("language".to_string(), "multi".to_string())]);

        let mut q2 = vec![];
        apply_language(&mut q2, None, "nova-2");
        assert!(q2.is_empty());

        let mut q3 = vec![];
        apply_language(&mut q3, Some("en"), "nova-3");
        assert_eq!(q3, vec![("language".to_string(), "en".to_string())]);

        // `detect` -> single-language auto-detect; `multi` -> code-switching (any model).
        let mut q4 = vec![];
        apply_language(&mut q4, Some("detect"), "nova-3");
        assert_eq!(q4, vec![("detect_language".to_string(), "true".to_string())]);

        let mut q5 = vec![];
        apply_language(&mut q5, Some("multi"), "nova-2");
        assert_eq!(q5, vec![("language".to_string(), "multi".to_string())]);
    }

    #[test]
    fn parse_keyterms_splits_on_commas_and_newlines() {
        assert_eq!(
            parse_keyterms(Some("Meetily, Deepgram\nNova")),
            vec!["Meetily", "Deepgram", "Nova"]
        );
        assert!(parse_keyterms(None).is_empty());
        assert!(parse_keyterms(Some("  ,\n ")).is_empty());
    }

    #[test]
    fn realtime_url_contains_expected_params() {
        let options = DeepgramOptions {
            model: "nova-3".to_string(),
            keyterms: vec!["Meetily".to_string()],
            diarize: true,
            mip_opt_out: true,
            language: None,
        };
        let url = build_realtime_url(&options, Some("en".to_string()));
        assert!(url.starts_with("wss://api.deepgram.com/v1/listen?"));
        assert!(url.contains("encoding=linear16"));
        assert!(url.contains("model=nova-3"));
        assert!(url.contains("diarize_model=latest"));
        assert!(!url.contains("diarize=true")); // diarize_model replaces diarize (400 if both)
        assert!(url.contains("mip_opt_out=true")); // privacy-first: opt out by default
        assert!(url.contains("interim_results=false"));
        assert!(url.contains("keyterm=Meetily"));
        assert!(url.contains("language=en"));
    }

    #[test]
    fn prerecorded_query_emits_diarize_model_and_mip_opt_out() {
        let client = DeepgramClient::new(
            "k".to_string(),
            DeepgramOptions {
                model: "nova-3".to_string(),
                keyterms: vec![],
                diarize: true,
                mip_opt_out: true,
                language: None,
            },
        );
        let q = client.prerecorded_query(Some("es".to_string()));
        let has = |k: &str, v: &str| q.iter().any(|(a, b)| a == k && b == v);
        assert!(has("diarize_model", "latest"));
        assert!(!q.iter().any(|(k, _)| k == "diarize")); // never diarize=true (400 if both)
        assert!(has("mip_opt_out", "true"));
        assert!(has("smart_format", "true"));
        assert!(has("language", "es")); // caller hint used when options.language is unset
    }

    #[test]
    fn deepgram_specific_language_overrides_caller_hint() {
        let opts = DeepgramOptions {
            model: "nova-3".to_string(),
            keyterms: vec![],
            diarize: false,
            mip_opt_out: false,
            language: Some("multi".to_string()),
        };
        // Even when the caller passes a global hint ("es"), the Deepgram-specific
        // language ("multi") wins, so global values never override the provider setting.
        let url = build_realtime_url(&opts, Some("es".to_string()));
        assert!(url.contains("language=multi"));
        assert!(!url.contains("language=es"));
    }

    #[test]
    fn language_auto_is_treated_as_no_hint() {
        // "auto" is treated as no explicit hint; nova-3 then falls back to multi.
        let mut q = vec![];
        apply_language(&mut q, Some("auto"), "nova-3");
        assert_eq!(q, vec![("language".to_string(), "multi".to_string())]);
    }

    #[test]
    fn keyterm_with_spaces_is_percent_encoded_in_url() {
        let options = DeepgramOptions {
            model: "nova-3".to_string(),
            keyterms: vec!["Acme Corp".to_string()],
            diarize: false,
            mip_opt_out: false,
            language: None,
        };
        let url = build_realtime_url(&options, None);
        assert!(url.contains("keyterm=Acme%20Corp"));
        assert!(!url.contains("diarize="));
        assert!(!url.contains("mip_opt_out")); // off -> param omitted
    }

    // Contract tests: lock the Deepgram wire shape we verified live so a
    // response-schema change is caught here rather than at runtime.
    #[test]
    fn parses_prerecorded_utterances_into_segments() {
        let json = r#"{
            "metadata": { "duration": 3.5 },
            "results": {
                "channels": [ { "alternatives": [ {
                    "transcript": "Hello there. General Kenobi.",
                    "confidence": 0.98,
                    "words": [ { "word": "hello", "punctuated_word": "Hello.",
                        "start": 0.0, "end": 0.8, "confidence": 0.95, "speaker": 0 } ]
                } ] } ],
                "utterances": [
                    { "start": 0.0, "end": 1.5, "confidence": 0.97,
                      "transcript": "Hello there.", "speaker": 0 },
                    { "start": 2.0, "end": 3.0, "confidence": 0.96,
                      "transcript": "General Kenobi.", "speaker": 1 }
                ]
            }
        }"#;
        let payload: PrerecordedResponse = serde_json::from_str(json).unwrap();
        assert_eq!(payload.metadata.and_then(|m| m.duration), Some(3.5));
        let transcript = DeepgramTranscript {
            text: payload.results.channels[0].alternatives[0].transcript.clone(),
            utterances: payload.results.utterances,
            words: payload.results.channels[0].alternatives[0].words.clone(),
            duration_seconds: Some(3.5),
        };
        let segments = transcript_to_segments(&transcript, 3.5);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Hello there.");
        assert_eq!(segments[1].audio_start_time, Some(2.0));
    }

    #[test]
    fn parses_realtime_results_message() {
        let json = r#"{
            "type": "Results",
            "channel_index": [0, 1],
            "duration": 1.02,
            "start": 4.5,
            "is_final": true,
            "speech_final": true,
            "channel": { "alternatives": [ {
                "transcript": "This is a meeting.",
                "confidence": 0.99,
                "words": [ { "word": "this", "punctuated_word": "This",
                    "start": 4.5, "end": 4.7, "confidence": 0.99, "speaker": 0 } ]
            } ] }
        }"#;
        let msg: RealtimeMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.msg_type.as_deref(), Some("Results"));
        assert!(msg.is_final);
        assert_eq!(msg.start, 4.5);
        let alt = msg.channel.unwrap().alternatives.into_iter().next().unwrap();
        assert_eq!(alt.transcript, "This is a meeting.");
        assert_eq!(alt.confidence, Some(0.99));
    }

    #[test]
    fn parses_realtime_error_message() {
        let json = r#"{ "type": "Error", "description": "bad audio", "message": "detail" }"#;
        let msg: RealtimeMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.msg_type.as_deref(), Some("Error"));
        assert_eq!(msg.description.as_deref(), Some("bad audio"));
    }

    #[test]
    fn nova_result_reduction_finals_only() {
        let mk = |is_final: bool| RealtimeMessage {
            msg_type: Some("Results".to_string()),
            channel: Some(RealtimeChannel {
                alternatives: vec![Alternative {
                    transcript: "Hi there.".to_string(),
                    confidence: Some(0.9),
                    words: vec![],
                }],
            }),
            is_final,
            start: 1.0,
            duration: 0.5,
            description: None,
            message: None,
        };
        assert!(nova_result_to_final(mk(false)).is_none());
        let seg = nova_result_to_final(mk(true)).unwrap();
        assert_eq!(seg.transcript, "Hi there.");
        assert_eq!(seg.start_ms, 1000.0);
        assert_eq!(seg.end_ms, 1500.0);
    }

    // --- Flux (v2) contract + reduction ---

    #[test]
    fn flux_reduction_only_on_end_of_turn() {
        let mk = |event: &str, transcript: &str| FluxTurnInfo {
            msg_type: Some("TurnInfo".to_string()),
            event: Some(event.to_string()),
            transcript: transcript.to_string(),
            audio_window_start: 0.0,
            audio_window_end: 8.32,
            words: vec![DeepgramWord {
                word: "Hello".to_string(),
                punctuated_word: None,
                start: 0.0,
                end: 0.48,
                confidence: Some(1.0),
                speaker: None,
            }],
            description: None,
            message: None,
        };
        assert!(flux_turn_to_final(&mk("Update", "Hello")).is_none());
        assert!(flux_turn_to_final(&mk("StartOfTurn", "Hello")).is_none());
        assert!(flux_turn_to_final(&mk("EndOfTurn", "   ")).is_none());
        let seg = flux_turn_to_final(&mk("EndOfTurn", "Hello. This is a test.")).unwrap();
        assert_eq!(seg.transcript, "Hello. This is a test.");
        assert_eq!(seg.start_ms, 0.0);
        assert_eq!(seg.end_ms, 8320.0);
    }

    #[test]
    fn parses_real_flux_turninfo_shape() {
        // Shape captured live from wss://api.deepgram.com/v2/listen (flux-general-multi).
        let json = r#"{
            "type": "TurnInfo", "request_id": "abc", "event": "EndOfTurn",
            "turn_index": 0, "audio_window_start": 0.0, "audio_window_end": 8.32,
            "transcript": "Hello. This is a flux test.",
            "words": [ { "word": "Hello.", "confidence": 1.0, "start": 0.0, "end": 0.48, "language": "en" } ],
            "languages": ["en"], "languages_hinted": ["en"],
            "end_of_turn_confidence": 0.83, "sequence_id": 12
        }"#;
        let m: FluxTurnInfo = serde_json::from_str(json).unwrap();
        assert_eq!(m.msg_type.as_deref(), Some("TurnInfo"));
        assert_eq!(m.event.as_deref(), Some("EndOfTurn"));
        let seg = flux_turn_to_final(&m).unwrap();
        assert_eq!(seg.transcript, "Hello. This is a flux test.");
    }

    #[test]
    fn is_flux_model_detects_flux() {
        assert!(is_flux_model("flux-general-en"));
        assert!(is_flux_model("flux-general-multi"));
        assert!(!is_flux_model("nova-3"));
        assert!(!is_flux_model("nova-2"));
    }

    #[test]
    fn flux_url_uses_v2_and_omits_nova_knobs() {
        let opts = DeepgramOptions {
            model: "flux-general-multi".to_string(),
            keyterms: vec!["Meetily".to_string()],
            diarize: true, // ignored by Flux
            mip_opt_out: false,
            language: None,
        };
        let url = build_flux_url(&opts, Some("en".to_string()));
        assert!(url.starts_with("wss://api.deepgram.com/v2/listen?"));
        assert!(url.contains("model=flux-general-multi"));
        assert!(url.contains("language_hint=en")); // multi model biases via hint
        assert!(url.contains("keyterm=Meetily")); // Flux uses keyterm, not keywords
        assert!(!url.contains("smart_format"));
        assert!(!url.contains("diarize"));
        assert!(!url.contains("endpointing"));
        assert!(!url.contains("language=")); // no language tag; model selects language
    }

    #[test]
    fn flux_en_url_omits_language_hint() {
        let opts = DeepgramOptions {
            model: "flux-general-en".to_string(),
            keyterms: vec![],
            diarize: false,
            mip_opt_out: false,
            language: None,
        };
        let url = build_flux_url(&opts, Some("en".to_string()));
        assert!(!url.contains("language_hint"));
    }

    #[test]
    fn prerecorded_falls_back_to_nova_for_flux() {
        // Flux is realtime-only; file transcription must use a nova model.
        let flux = DeepgramClient::new(
            "k".to_string(),
            DeepgramOptions {
                model: "flux-general-en".to_string(),
                keyterms: vec!["Meetily".to_string()],
                diarize: true,
                mip_opt_out: false,
                language: None,
            },
        );
        let q = flux.prerecorded_query(None);
        let model = &q.iter().find(|(k, _)| k == "model").unwrap().1;
        assert_eq!(model, crate::config::DEFAULT_DEEPGRAM_ASYNC_MODEL);
        assert!(!is_flux_model(model));
        // keyterm still routes correctly for the nova fallback model.
        assert!(q.iter().any(|(k, _)| k == "keyterm"));
    }
}
