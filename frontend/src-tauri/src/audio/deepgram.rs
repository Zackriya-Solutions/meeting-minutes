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

use super::audio_processing::{deinterleave, interleave_stereo};
use super::recording_state::{AudioChunk, AudioLayout};
use super::transcription::TranscriptUpdate;

pub const PROVIDER: &str = "deepgram";
const LISTEN_REST_URL: &str = "https://api.deepgram.com/v1/listen";
const LISTEN_WS_URL: &str = "wss://api.deepgram.com/v1/listen";
// Flux uses a separate v2 realtime endpoint with turn-based events.
const FLUX_WS_URL: &str = "wss://api.deepgram.com/v2/listen";
const PCM_SAMPLE_RATE: u32 = 16_000;
/// Deepgram closes an idle realtime socket after ~10s; send KeepAlive well under that.
const KEEPALIVE_SECS: u64 = 5;
/// Flux end-of-stream: 20 x 100 ms of silence (2 s) lets the last turn close.
const FLUX_SILENCE_FRAME_SAMPLES: usize = (PCM_SAMPLE_RATE / 10) as usize;
const FLUX_TRAILING_SILENCE_FRAMES: usize = 20;

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
    /// `[channel, total_channels]` (`[0, 1]` on mono). Kept as raw JSON and
    /// read by `parse_channel_index`, so an unexpected shape cannot reject the
    /// whole message.
    #[serde(default)]
    channel_index: serde_json::Value,
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

/// A final nova `Results` message: which channel it covers, how far that
/// channel is now finalized (stream ms), and its segment if it had words.
/// Empty finals still matter: they advance the channel's watermark.
struct NovaFinal {
    channel_index: Option<(usize, usize)>,
    finalized_through_ms: f64,
    seg: Option<FinalSeg>,
}

/// Reduce a nova message to a final (None for interim and non-Results).
fn nova_final(m: RealtimeMessage) -> Option<NovaFinal> {
    if m.msg_type.as_deref() != Some("Results") || !m.is_final {
        return None;
    }
    let channel_index = parse_channel_index(&m.channel_index);
    let start_ms = m.start * 1000.0;
    let end_ms = (m.start + m.duration) * 1000.0;
    let seg = m
        .channel
        .and_then(|c| c.alternatives.into_iter().next())
        .and_then(|alternative| {
            let transcript = alternative.transcript.trim();
            if transcript.is_empty() {
                return None;
            }
            let confidence = alternative.confidence.unwrap_or_else(|| {
                average_confidence(alternative.words.iter().filter_map(|w| w.confidence))
            });
            Some(FinalSeg { transcript: transcript.to_string(), start_ms, end_ms, confidence })
        });
    Some(NovaFinal { channel_index, finalized_through_ms: end_ms, seg })
}

/// Reduce a nova `Results` message to a finalized segment (None for interim/other).
/// Test shorthand: the realtime reader uses `nova_final` for the progress too.
#[cfg(test)]
fn nova_result_to_final(m: RealtimeMessage) -> Option<FinalSeg> {
    nova_final(m)?.seg
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
            // Prerecorded files are transcribed as one mixed signal.
            speaker: None,
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
// SOURCE LABELS
// ============================================================================

/// Transcript label for the microphone channel.
/// DO NOT change without frontend/src/lib/transcript-speakers.ts
/// (`ts_speaker_labels_match_rust_constants` enforces they match).
pub const MIC_IN_LABEL: &str = "Mic-In";
/// Transcript label for the system-output channel. Same constraint as above.
pub const SPEAKER_OUT_LABEL: &str = "Speaker-Out";

/// The single mapping from a realtime channel to its transcript label.
/// DO NOT reorder: the pipeline interleaves stereo frames as [mic, system]
/// (pipeline.rs `ContinuousStereo`), so channel 0 is the mic. A `Mono` stream
/// has no single source and gets no label.
pub fn channel_label(layout: AudioLayout, channel: usize) -> Option<&'static str> {
    match (layout, channel) {
        (AudioLayout::MicOnly, 0) | (AudioLayout::MicSystemStereo, 0) => Some(MIC_IN_LABEL),
        (AudioLayout::MicSystemStereo, 1) => Some(SPEAKER_OUT_LABEL),
        _ => None,
    }
}

/// The global channel carrying the far side, whose progress gates held Mic-In
/// finals (see `BleedSuppressor`).
const SPEAKER_OUT_CHANNEL: usize = 1;

/// One realtime WebSocket and the input channels it carries. nova takes every
/// channel on a single socket (`channels=N&multichannel=true`). Flux has no
/// multichannel support (`/v2/listen` answers HTTP 400 "Unknown query
/// parameters: channels, multichannel", verified live), so it gets one mono
/// socket per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Lane {
    first_channel: usize,
    channels: usize,
}

fn plan_lanes(flux: bool, input_channels: usize) -> Vec<Lane> {
    let input_channels = input_channels.clamp(1, 2);
    if flux {
        (0..input_channels)
            .map(|first_channel| Lane { first_channel, channels: 1 })
            .collect()
    } else {
        vec![Lane { first_channel: 0, channels: input_channels }]
    }
}

/// Global input channel of a result, from the lane and the `channel_index`
/// Deepgram reported. A mono socket is unambiguous. On a multichannel socket a
/// missing or out-of-range index yields None: the line is emitted unlabelled
/// rather than guessed (a guess of 0 would call the far side "Mic-In").
fn resolve_channel(lane: Lane, reported: Option<(usize, usize)>) -> Option<usize> {
    if lane.channels == 1 {
        return Some(lane.first_channel);
    }
    match reported {
        Some((channel, total)) if channel < lane.channels && total == lane.channels => {
            Some(lane.first_channel + channel)
        }
        _ => None,
    }
}

/// Lenient read of `channel_index` (`[channel, total]`). Any other shape is
/// None instead of failing the whole Results message, which would drop its
/// transcript.
fn parse_channel_index(value: &serde_json::Value) -> Option<(usize, usize)> {
    match value.as_array()?.as_slice() {
        [channel, total] => Some((channel.as_u64()? as usize, total.as_u64()? as usize)),
        _ => None,
    }
}

/// Resampled channels may differ by a sample or two (rounding); more than this
/// means the conversion went wrong and the chunk is dropped.
const MAX_CHANNEL_LENGTH_SKEW: usize = 2;

/// Convert one pipeline chunk into a lane's 16 kHz samples, interleaved when
/// the lane carries two channels. Each channel is resampled on its own:
/// resampling interleaved data would smear the two sources into each other.
/// None means the chunk must be dropped (logged): a channel failed to
/// resample, or the channels came out at different lengths. Sending audio at
/// the wrong rate, or interleaving mismatched rates, would garble the
/// transcript and skew every later timestamp.
fn lane_samples(chunk: &AudioChunk, lane: Lane) -> Option<Vec<f32>> {
    lane_samples_with(chunk, lane, |samples, from, to| {
        super::audio_processing::resample(samples, from, to)
    })
}

fn lane_samples_with<F>(chunk: &AudioChunk, lane: Lane, resample: F) -> Option<Vec<f32>>
where
    F: Fn(&[f32], u32, u32) -> Result<Vec<f32>>,
{
    let mut picked = Vec::with_capacity(lane.channels);
    for (offset, samples) in deinterleave(&chunk.data, chunk.layout.channels())
        .into_iter()
        .skip(lane.first_channel)
        .take(lane.channels)
        .enumerate()
    {
        if chunk.sample_rate == PCM_SAMPLE_RATE {
            picked.push(samples);
            continue;
        }
        match resample(&samples, chunk.sample_rate, PCM_SAMPLE_RATE) {
            Ok(resampled) => picked.push(resampled),
            Err(e) => {
                error!(
                    "Deepgram realtime: dropping chunk {}: channel {} failed to resample {} -> {} Hz: {}",
                    chunk.chunk_id,
                    lane.first_channel + offset,
                    chunk.sample_rate,
                    PCM_SAMPLE_RATE,
                    e
                );
                return None;
            }
        }
    }
    match picked.len() {
        0 => Some(Vec::new()),
        1 => picked.pop(),
        _ => {
            let (left, right) = (&picked[0], &picked[1]);
            let skew = left.len().abs_diff(right.len());
            if skew > MAX_CHANNEL_LENGTH_SKEW {
                warn!(
                    "Deepgram realtime: dropping chunk {}: channels resampled to {} vs {} samples",
                    chunk.chunk_id,
                    left.len(),
                    right.len()
                );
                return None;
            }
            Some(interleave_stereo(left, right))
        }
    }
}

/// Recording-relative duration of a chunk in ms, counted in frames so an
/// interleaved stereo chunk is not reported as twice its real length.
fn chunk_duration_ms(chunk: &AudioChunk) -> f64 {
    let frames = chunk.data.len() / chunk.layout.channels() as usize;
    frames as f64 / chunk.sample_rate as f64 * 1000.0
}

// ============================================================================
// BLEED SUPPRESSION
// ============================================================================

/// A held Mic-In final is released once the Speaker-Out channel has finalized
/// audio this far past the mic segment's end: by then any Speaker-Out final
/// that could contain the same words has arrived.
const BLEED_WATERMARK_MARGIN_MS: f64 = 500.0;
/// Safety cap so a stalled Speaker-Out channel cannot hold mic lines forever.
const BLEED_MAX_HOLD_MS: f64 = 15_000.0;
/// Recording-time slack when deciding two segments are the same utterance.
const BLEED_MAX_GAP_MS: f64 = 2000.0;
/// Token Jaccard at or above which two segments are the same words.
const BLEED_MIN_JACCARD: f64 = 0.8;
/// For a partial copy (the channels segment differently), the share of the
/// shorter segment's tokens found in the longer one, if it has enough tokens
/// that a coincidental overlap is implausible.
const BLEED_MIN_CONTAINMENT: f64 = 0.9;
const BLEED_MIN_CONTAINMENT_TOKENS: usize = 4;
/// Segments shorter than this ("Okay.", "Yeah, sure.") match genuine replies
/// too easily, so they also have to coincide in time: acoustic bleed is
/// near-instant, a reply comes after the far side.
const BLEED_SHORT_TOKENS: usize = 3;
const BLEED_SHORT_MAX_START_DELTA_MS: f64 = 300.0;
const BLEED_SHORT_MIN_OVERLAP: f64 = 0.5;
/// How long Speaker-Out finals are remembered for matching late Mic-In finals.
const SPEAKER_MEMORY_MS: f64 = 30_000.0;

/// A finalized realtime segment in recording-relative ms, ready to emit.
#[derive(Debug, Clone, PartialEq)]
struct LabeledSegment {
    text: String,
    start_ms: f64,
    end_ms: f64,
    confidence: f32,
    speaker: Option<&'static str>,
}

fn word_tokens(text: &str) -> std::collections::HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect()
}

/// True when `mic` is the far side's audio leaking into the microphone
/// (speakers into the mic, or an echo path) rather than the user speaking.
/// Measured on a speakers-on Slack huddle: 82 of 109 mic segments were
/// verbatim copies of a concurrent system-audio segment.
fn is_bleed(mic: &LabeledSegment, speaker: &LabeledSegment) -> bool {
    let close_in_time = mic.start_ms <= speaker.end_ms + BLEED_MAX_GAP_MS
        && speaker.start_ms <= mic.end_ms + BLEED_MAX_GAP_MS;
    if !close_in_time {
        return false;
    }
    let a = word_tokens(&mic.text);
    let b = word_tokens(&speaker.text);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let shared = a.intersection(&b).count() as f64;
    let jaccard = shared / a.union(&b).count() as f64;
    let smaller = a.len().min(b.len());
    let same_words = jaccard >= BLEED_MIN_JACCARD
        || (smaller >= BLEED_MIN_CONTAINMENT_TOKENS && shared / smaller as f64 >= BLEED_MIN_CONTAINMENT);
    if !same_words {
        return false;
    }
    if smaller < BLEED_SHORT_TOKENS {
        let start_delta = (mic.start_ms - speaker.start_ms).abs();
        let overlap = (mic.end_ms.min(speaker.end_ms) - mic.start_ms.max(speaker.start_ms)).max(0.0);
        let mic_duration = (mic.end_ms - mic.start_ms).max(1.0);
        return start_delta <= BLEED_SHORT_MAX_START_DELTA_MS
            && overlap / mic_duration >= BLEED_SHORT_MIN_OVERLAP;
    }
    true
}

/// Drops Mic-In segments that duplicate a Speaker-Out segment, whichever
/// arrives first. A Mic-In final is held until the Speaker-Out channel has
/// finalized past it (a watermark), not for a fixed time: the two channels
/// finalize independently and a long far-side turn can finish well after the
/// mic copy of its first words. Pure: the caller supplies a monotonic clock.
#[derive(Default)]
struct BleedSuppressor {
    pending_mic: Vec<(f64, LabeledSegment)>,
    recent_speaker: Vec<(f64, LabeledSegment)>,
    /// Recording ms up to which the Speaker-Out channel has no pending audio.
    speaker_finalized_ms: f64,
}

impl BleedSuppressor {
    /// Accept a final; returns the segments that are now ready to emit.
    fn push(&mut self, seg: LabeledSegment, now_ms: f64) -> Vec<LabeledSegment> {
        let mut ready = Vec::new();
        match seg.speaker {
            Some(MIC_IN_LABEL) => {
                let duplicate = self.recent_speaker.iter().any(|(_, s)| is_bleed(&seg, s));
                if !duplicate {
                    self.pending_mic.push((now_ms, seg));
                }
            }
            Some(SPEAKER_OUT_LABEL) => {
                self.pending_mic.retain(|(_, m)| !is_bleed(m, &seg));
                self.speaker_finalized_ms = self.speaker_finalized_ms.max(seg.end_ms);
                self.recent_speaker.push((now_ms, seg.clone()));
                ready.push(seg);
            }
            _ => ready.push(seg),
        }
        ready.extend(self.drain_ready(now_ms));
        ready
    }

    /// The Speaker-Out channel has finalized everything before `recording_ms`
    /// (including stretches that produced no words).
    fn speaker_progress(&mut self, recording_ms: f64, now_ms: f64) -> Vec<LabeledSegment> {
        self.speaker_finalized_ms = self.speaker_finalized_ms.max(recording_ms);
        self.drain_ready(now_ms)
    }

    /// Release Mic-In segments the Speaker-Out channel has finalized past, or
    /// that hit the safety cap.
    fn drain_ready(&mut self, now_ms: f64) -> Vec<LabeledSegment> {
        self.recent_speaker
            .retain(|(at, _)| now_ms - at <= SPEAKER_MEMORY_MS);
        let watermark = self.speaker_finalized_ms;
        let (ready, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending_mic)
            .into_iter()
            .partition(|(at, seg)| {
                watermark >= seg.end_ms + BLEED_WATERMARK_MARGIN_MS || now_ms - at >= BLEED_MAX_HOLD_MS
            });
        self.pending_mic = waiting;
        ready.into_iter().map(|(_, seg)| seg).collect()
    }

    /// Release everything still held (end of stream).
    fn flush(&mut self) -> Vec<LabeledSegment> {
        std::mem::take(&mut self.pending_mic)
            .into_iter()
            .map(|(_, seg)| seg)
            .collect()
    }
}

/// Tracks how far a Flux socket has finalized: while a turn is open its audio
/// is still pending from the turn's start; between turns everything up to the
/// latest window end is final.
#[derive(Default)]
struct FluxProgress {
    open_turn_start_ms: Option<f64>,
}

impl FluxProgress {
    /// Observe a TurnInfo; returns stream ms through which audio is finalized.
    fn observe(&mut self, m: &FluxTurnInfo) -> f64 {
        let start_ms = m.audio_window_start * 1000.0;
        let end_ms = m.audio_window_end * 1000.0;
        match m.event.as_deref() {
            Some("EndOfTurn") => self.open_turn_start_ms = None,
            Some("StartOfTurn") | Some("TurnResumed") => {
                self.open_turn_start_ms.get_or_insert(start_ms);
            }
            _ if !m.transcript.trim().is_empty() => {
                self.open_turn_start_ms.get_or_insert(start_ms);
            }
            _ => {}
        }
        self.open_turn_start_ms.unwrap_or(end_ms)
    }
}

// ============================================================================
// REALTIME (WebSocket)
// ============================================================================

/// Where the realtime sockets connect. Production uses `DEEPGRAM`; tests point
/// it at a local server to exercise failure paths.
#[derive(Debug, Clone)]
struct RealtimeEndpoints {
    nova: String,
    flux: String,
}

impl RealtimeEndpoints {
    fn deepgram() -> Self {
        Self { nova: LISTEN_WS_URL.to_string(), flux: FLUX_WS_URL.to_string() }
    }
}

/// One chunk's worth of audio for one lane, plus where it sits in the recording.
struct LaneFrame {
    bytes: Vec<u8>,
    entry: AudioTimeMapEntry,
}

/// What lane readers report to the emitter, in arrival order.
enum LaneEvent {
    Final(LabeledSegment),
    /// The Speaker-Out channel is finalized through this recording ms.
    SpeakerProgress(f64),
}

pub async fn run_realtime_transcription<R: Runtime>(
    app: AppHandle<R>,
    transcription_receiver: mpsc::UnboundedReceiver<AudioChunk>,
) -> Result<(), String> {
    REALTIME_SPEECH_DETECTED_EMITTED.store(false, Ordering::SeqCst);

    let api_key = configured_api_key(&app).await.map_err(|e| e.to_string())?;
    let options = configured_options(&app).await.map_err(|e| e.to_string())?;
    let language = crate::get_language_preference_internal();

    let emitter_app = app.clone();
    run_realtime_core(
        RealtimeEndpoints::deepgram(),
        api_key,
        options,
        language,
        transcription_receiver,
        move |seg| emit_labeled_segment(&emitter_app, seg),
    )
    .await
}

/// The realtime session itself, free of Tauri state so it can be driven
/// directly (see `live_stereo_realtime_labels_each_channel`).
/// `emit` receives every finalized segment after bleed suppression. It is
/// never called after this function returns, on success or error: the
/// worker emits `transcription-error` after an Err, and a late transcript
/// line after that would be out of order.
async fn run_realtime_core<F>(
    endpoints: RealtimeEndpoints,
    api_key: String,
    options: DeepgramOptions,
    language: Option<String>,
    transcription_receiver: mpsc::UnboundedReceiver<AudioChunk>,
    emit: F,
) -> Result<(), String>
where
    F: Fn(LabeledSegment) + Send + 'static,
{
    // The socket URL needs the channel count, and only the pipeline knows it,
    // so wait for the first audio before connecting. Audio queues in the
    // channel meanwhile; nothing is lost.
    let mut receiver = transcription_receiver;
    let first_chunk = loop {
        match receiver.recv().await {
            Some(chunk) if !chunk.data.is_empty() => break chunk,
            Some(_) => continue,
            None => {
                info!("Deepgram realtime: recording ended before any audio arrived");
                return Ok(());
            }
        }
    };
    let layout = first_chunk.layout;
    let input_channels = layout.channels() as usize;

    // Flux is a distinct realtime API: /v2/listen with turn-based `TurnInfo` events
    // and its own model-selected language handling. nova uses /v1/listen `Results`.
    let flux = is_flux_model(&options.model);
    let lanes = plan_lanes(flux, input_channels);
    info!(
        "Connecting to Deepgram realtime transcription ({}, {:?}, {} socket(s))",
        options.model,
        layout,
        lanes.len()
    );

    // Connect every lane before spawning anything, so a failed connection
    // (e.g. the second Flux socket) leaves no running task behind.
    let mut sockets = Vec::with_capacity(lanes.len());
    for lane in &lanes {
        let url = if flux {
            build_flux_url(&endpoints.flux, &options, language.clone())
        } else {
            build_realtime_url(&endpoints.nova, &options, language.clone(), lane.channels)
        };
        let mut request = url
            .into_client_request()
            .map_err(|e| format!("Failed to build Deepgram WebSocket request: {}", e))?;
        // Deepgram uses the "Token" scheme, not "Bearer".
        let auth_header = HeaderValue::from_str(&format!("Token {}", api_key))
            .map_err(|e| format!("Failed to build Deepgram auth header: {}", e))?;
        request.headers_mut().insert("Authorization", auth_header);
        let (ws_stream, _) = connect_async(request)
            .await
            .map_err(|e| format!("Failed to connect to Deepgram realtime API: {}", e))?;
        sockets.push((*lane, ws_stream));
    }

    let shutdown = Arc::new(AtomicBool::new(false));
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<LaneEvent>();
    let mut lane_senders = Vec::with_capacity(lanes.len());
    let mut writer_handles = Vec::with_capacity(lanes.len());
    let mut reader_handles = Vec::with_capacity(lanes.len());

    for (lane, ws_stream) in sockets {
        let (mut write, mut read) = ws_stream.split();
        let time_map = Arc::new(Mutex::new(Vec::<AudioTimeMapEntry>::new()));
        let (lane_tx, mut lane_rx) = mpsc::unbounded_channel::<LaneFrame>();
        lane_senders.push((lane, lane_tx));

        let writer_time_map = time_map.clone();
        let writer_shutdown = shutdown.clone();
        writer_handles.push(tokio::spawn(async move {
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
                        if !flux {
                            let msg = serde_json::json!({ "type": "KeepAlive" }).to_string();
                            if let Err(e) = write.send(Message::Text(msg)).await {
                                return Err(anyhow!("Failed to send Deepgram KeepAlive: {}", e));
                            }
                        }
                    }
                    maybe_frame = lane_rx.recv() => {
                        let Some(frame) = maybe_frame else {
                            break;
                        };
                        writer_time_map.lock().await.push(frame.entry);
                        if let Err(e) = write.send(Message::Binary(frame.bytes)).await {
                            return Err(anyhow!("Failed to send audio to Deepgram realtime API: {}", e));
                        }
                    }
                }
            }

            // Flux only ends a turn after it hears silence: neither CloseStream nor a
            // WebSocket close flushes the in-progress turn (verified live), and
            // dropping the write half leaves the socket open until Deepgram's 60s
            // no-ping timeout. So feed trailing silence at realtime pace first,
            // unless the session is already failing.
            if flux && !writer_shutdown.load(Ordering::SeqCst) {
                let silence = f32_to_pcm16le(&vec![0.0; FLUX_SILENCE_FRAME_SAMPLES * lane.channels]);
                for _ in 0..FLUX_TRAILING_SILENCE_FRAMES {
                    if write.send(Message::Binary(silence.clone())).await.is_err() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
            // Both protocols then finalize and close on CloseStream.
            let close = serde_json::json!({ "type": "CloseStream" }).to_string();
            let _ = write.send(Message::Text(close)).await;
            Ok::<(), anyhow::Error>(())
        }));

        let reader_time_map = time_map;
        let reader_shutdown = shutdown.clone();
        let reader_events = event_tx.clone();
        reader_handles.push(tokio::spawn(async move {
            let mut flux_progress = FluxProgress::default();
            while let Some(message) = read.next().await {
                let message = match message {
                    Ok(message) => message,
                    Err(e) => {
                        reader_shutdown.store(true, Ordering::SeqCst);
                        return Err(anyhow!("Deepgram realtime WebSocket error: {}", e));
                    }
                };
                let Message::Text(text) = message else {
                    if matches!(message, Message::Close(_)) {
                        break;
                    }
                    continue;
                };

                // Reduce per protocol to (global channel, finalized-through ms,
                // optional final). Both report `type:"Error"` on failure.
                let (channel, finalized_through_ms, final_seg) = if flux {
                    let m = match serde_json::from_str::<FluxTurnInfo>(&text) {
                        Ok(m) => m,
                        Err(e) => {
                            warn!("Failed to parse Deepgram Flux response: {} ({})", e, text);
                            continue;
                        }
                    };
                    if m.msg_type.as_deref() == Some("Error") {
                        let err = m.description.or(m.message).unwrap_or_else(|| "unknown error".to_string());
                        reader_shutdown.store(true, Ordering::SeqCst);
                        return Err(anyhow!("Deepgram Flux realtime error: {}", err));
                    }
                    if m.msg_type.as_deref() != Some("TurnInfo") {
                        continue;
                    }
                    let through = flux_progress.observe(&m);
                    (resolve_channel(lane, None), through, flux_turn_to_final(&m))
                } else {
                    let m = match serde_json::from_str::<RealtimeMessage>(&text) {
                        Ok(m) => m,
                        Err(e) => {
                            warn!("Failed to parse Deepgram realtime response: {} ({})", e, text);
                            continue;
                        }
                    };
                    if m.msg_type.as_deref() == Some("Error") {
                        let err = m.description.or(m.message).unwrap_or_else(|| "unknown error".to_string());
                        reader_shutdown.store(true, Ordering::SeqCst);
                        return Err(anyhow!("Deepgram realtime error: {}", err));
                    }
                    let Some(fin) = nova_final(m) else { continue };
                    let channel = resolve_channel(lane, fin.channel_index);
                    if channel.is_none() {
                        warn!(
                            "Deepgram realtime: Results on a {}-channel socket with channel_index {:?}; emitting unlabelled",
                            lane.channels, fin.channel_index
                        );
                    }
                    (channel, fin.finalized_through_ms, fin.seg)
                };

                let entries = reader_time_map.lock().await.clone();
                if let Some(seg) = final_seg {
                    let start_ms = map_stream_ms_to_original(&entries, seg.start_ms);
                    let end_ms = map_stream_ms_to_original(&entries, seg.end_ms).max(start_ms);
                    let _ = reader_events.send(LaneEvent::Final(LabeledSegment {
                        text: seg.transcript,
                        start_ms,
                        end_ms,
                        confidence: seg.confidence,
                        speaker: channel.and_then(|c| channel_label(layout, c)),
                    }));
                }
                if layout == AudioLayout::MicSystemStereo && channel == Some(SPEAKER_OUT_CHANNEL) {
                    let through = map_stream_ms_to_original(&entries, finalized_through_ms);
                    let _ = reader_events.send(LaneEvent::SpeakerProgress(through));
                }
            }

            reader_shutdown.store(true, Ordering::SeqCst);
            Ok::<(), anyhow::Error>(())
        }));
    }
    // Only the readers hold senders now, so the emitter ends when they do.
    drop(event_tx);

    // Bleed is only possible when there is a Speaker-Out channel to copy from;
    // a mic-only or mono stream is emitted as it arrives.
    let suppress_bleed = layout == AudioLayout::MicSystemStereo;
    let emitter_handle = tokio::spawn(async move {
        let started = std::time::Instant::now();
        let now_ms = || started.elapsed().as_secs_f64() * 1000.0;
        let mut suppressor = BleedSuppressor::default();
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        loop {
            let ready = tokio::select! {
                maybe_event = event_rx.recv() => match maybe_event {
                    Some(LaneEvent::Final(seg)) if !suppress_bleed => vec![seg],
                    Some(LaneEvent::Final(seg)) => suppressor.push(seg, now_ms()),
                    Some(LaneEvent::SpeakerProgress(ms)) => suppressor.speaker_progress(ms, now_ms()),
                    None => break,
                },
                _ = tick.tick() => suppressor.drain_ready(now_ms()),
            };
            for seg in ready {
                emit(seg);
            }
        }
        for seg in suppressor.flush() {
            emit(seg);
        }
    });

    // Dispatcher: fan each pipeline chunk out to every lane's socket.
    let mut first_error: Option<String> = None;
    {
        let mut cursors_ms = vec![0.0f64; lane_senders.len()];
        let mut next = Some(first_chunk);
        let mut shutdown_check = tokio::time::interval(Duration::from_millis(250));
        loop {
            let chunk = match next.take() {
                Some(chunk) => chunk,
                None => tokio::select! {
                    maybe_chunk = receiver.recv() => match maybe_chunk {
                        Some(chunk) => chunk,
                        None => break,
                    },
                    _ = shutdown_check.tick() => {
                        if shutdown.load(Ordering::SeqCst) {
                            break;
                        }
                        continue;
                    }
                },
            };
            if chunk.data.is_empty() {
                continue;
            }
            if chunk.layout != layout {
                warn!(
                    "Deepgram realtime: dropping {:?} chunk on a {:?} stream",
                    chunk.layout, layout
                );
                continue;
            }

            let original_start_ms = chunk.timestamp * 1000.0;
            let original_end_ms = original_start_ms + chunk_duration_ms(&chunk);

            for ((lane, lane_tx), cursor) in lane_senders.iter().zip(cursors_ms.iter_mut()) {
                let Some(samples) = lane_samples(&chunk, *lane) else {
                    continue;
                };
                if samples.is_empty() {
                    continue;
                }
                let frames = samples.len() / lane.channels;
                let duration_ms = frames as f64 / PCM_SAMPLE_RATE as f64 * 1000.0;
                let entry = AudioTimeMapEntry {
                    stream_start_ms: *cursor,
                    stream_end_ms: *cursor + duration_ms,
                    original_start_ms,
                    original_end_ms,
                };
                *cursor += duration_ms;
                if lane_tx
                    .send(LaneFrame { bytes: f32_to_pcm16le(&samples), entry })
                    .is_err()
                {
                    // The lane's writer already exited; its error surfaces below.
                    shutdown.store(true, Ordering::SeqCst);
                }
            }
        }
    }
    // Closing the lane senders tells each writer to finalize its socket.
    drop(lane_senders);

    for writer_handle in writer_handles {
        match writer_handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                first_error.get_or_insert_with(|| e.to_string());
            }
            Err(e) => {
                first_error.get_or_insert_with(|| format!("Deepgram realtime writer task failed: {}", e));
            }
        }
        if first_error.is_some() {
            shutdown.store(true, Ordering::SeqCst);
        }
    }

    // Readers: on success, give the final responses a shared 30s deadline
    // (waiting 30s per socket in turn would stack). On any error, stop every
    // reader now; nothing they would still produce may reach `emit`.
    let reader_aborts: Vec<_> = reader_handles.iter().map(|h| h.abort_handle()).collect();
    if first_error.is_some() {
        reader_aborts.iter().for_each(|h| h.abort());
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    for mut reader_handle in reader_handles {
        match tokio::time::timeout_at(deadline, &mut reader_handle).await {
            Ok(Ok(Ok(()))) => {}
            Ok(Ok(Err(e))) => {
                first_error.get_or_insert_with(|| e.to_string());
                reader_aborts.iter().for_each(|h| h.abort());
            }
            Ok(Err(e)) if e.is_cancelled() => {}
            Ok(Err(e)) => {
                first_error.get_or_insert_with(|| format!("Deepgram realtime reader task failed: {}", e));
                reader_aborts.iter().for_each(|h| h.abort());
            }
            Err(_) => {
                warn!("Timed out waiting for Deepgram realtime final response");
                reader_handle.abort();
                let _ = reader_handle.await;
            }
        }
    }

    // Every reader has ended, so the event channel is closed: the emitter
    // releases what the suppressor still holds and exits. Await it so no
    // emit can happen after we return.
    let _ = emitter_handle.await;

    match first_error {
        Some(e) => Err(e),
        None => {
            info!("Deepgram realtime transcription task completed");
            Ok(())
        }
    }
}

fn emit_labeled_segment<R: Runtime>(app: &AppHandle<R>, seg: LabeledSegment) {
    emit_speech_detected(app);
    emit_transcript_update(
        app,
        seg.text,
        seg.start_ms,
        seg.end_ms,
        seg.confidence,
        false,
        seg.speaker.map(str::to_string),
    );
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

fn build_realtime_url(base: &str, options: &DeepgramOptions, language: Option<String>, channels: usize) -> String {
    let channels = channels.max(1);
    let mut query: Vec<(String, String)> = vec![
        ("encoding".to_string(), "linear16".to_string()),
        ("sample_rate".to_string(), PCM_SAMPLE_RATE.to_string()),
        ("channels".to_string(), channels.to_string()),
        ("model".to_string(), options.model.clone()),
        ("smart_format".to_string(), "true".to_string()),
        ("punctuate".to_string(), "true".to_string()),
        // Finals only: the transcript UI buffers by sequence id with no
        // partial-replace path, so interim results would duplicate segments.
        ("interim_results".to_string(), "false".to_string()),
        ("endpointing".to_string(), "300".to_string()),
    ];
    if channels > 1 {
        // Transcribe each channel independently (results carry `channel_index`)
        // instead of downmixing; verified live with nova-3 streaming.
        query.push(("multichannel".to_string(), "true".to_string()));
    }
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
    encode_ws_url(base, &query)
}

/// Build the Flux v2 realtime URL. Flux does its own turn detection and
/// formatting, so it takes none of the nova knobs (smart_format/punctuate/
/// diarize/endpointing/language). The multilingual model accepts `language_hint`;
/// language is otherwise selected by the model name, not a `language` param.
fn build_flux_url(base: &str, options: &DeepgramOptions, language: Option<String>) -> String {
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
    encode_ws_url(base, &query)
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
    speaker: Option<String>,
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
        speaker,
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
        let url = build_realtime_url(LISTEN_WS_URL, &options, Some("en".to_string()), 1);
        assert!(url.starts_with("wss://api.deepgram.com/v1/listen?"));
        assert!(url.contains("encoding=linear16"));
        assert!(url.contains("model=nova-3"));
        assert!(url.contains("diarize_model=latest"));
        assert!(!url.contains("diarize=true")); // diarize_model replaces diarize (400 if both)
        assert!(url.contains("mip_opt_out=true")); // privacy-first: opt out by default
        assert!(url.contains("interim_results=false"));
        assert!(url.contains("keyterm=Meetily"));
        assert!(url.contains("language=en"));
        // Mono socket: one channel and no multichannel split.
        assert!(url.contains("channels=1"));
        assert!(!url.contains("multichannel"));
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
        let url = build_realtime_url(LISTEN_WS_URL, &opts, Some("es".to_string()), 1);
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
        let url = build_realtime_url(LISTEN_WS_URL, &options, None, 1);
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
            channel_index: serde_json::json!([0, 1]),
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
        let url = build_flux_url(FLUX_WS_URL, &opts, Some("en".to_string()));
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
        let url = build_flux_url(FLUX_WS_URL, &opts, Some("en".to_string()));
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

    // ------------------------------------------------------------------
    // Multichannel realtime: labels, lanes, channel parsing, time mapping
    // ------------------------------------------------------------------

    use super::super::recording_state::DeviceType;

    #[test]
    fn channel_labels_follow_layout() {
        use AudioLayout::*;
        assert_eq!(channel_label(MicSystemStereo, 0), Some("Mic-In"));
        assert_eq!(channel_label(MicSystemStereo, 1), Some("Speaker-Out"));
        // Mic-only recording: the one channel is the mic.
        assert_eq!(channel_label(MicOnly, 0), Some("Mic-In"));
        // A mixed or raw mono stream has no single source.
        assert_eq!(channel_label(Mono, 0), None);
        assert_eq!(channel_label(MicSystemStereo, 2), None);
    }

    /// Finding 7: the frontend keeps its own copy of the labels for styling and
    /// export. Read it and require exactly these two values.
    #[test]
    fn ts_speaker_labels_match_rust_constants() {
        let ts = include_str!("../../../src/lib/transcript-speakers.ts");
        let labels: Vec<(&str, &str)> = ts
            .lines()
            .filter_map(|line| {
                let rest = line.trim().strip_prefix("export const ")?;
                let (name, value) = rest.split_once(" = ")?;
                if !name.ends_with("_LABEL") {
                    return None;
                }
                Some((name, value.trim_end_matches(';').trim_matches('\'')))
            })
            .collect();
        assert_eq!(
            labels,
            vec![("MIC_IN_LABEL", MIC_IN_LABEL), ("SPEAKER_OUT_LABEL", SPEAKER_OUT_LABEL)]
        );
    }

    #[test]
    fn stereo_realtime_url_requests_multichannel() {
        let url = build_realtime_url(LISTEN_WS_URL, &DeepgramOptions::default(), None, 2);
        assert!(url.contains("channels=2"));
        assert!(url.contains("multichannel=true"));
    }

    #[test]
    fn nova_uses_one_socket_and_flux_one_socket_per_channel() {
        assert_eq!(plan_lanes(false, 2), vec![Lane { first_channel: 0, channels: 2 }]);
        assert_eq!(
            plan_lanes(true, 2),
            vec![
                Lane { first_channel: 0, channels: 1 },
                Lane { first_channel: 1, channels: 1 }
            ]
        );
        assert_eq!(plan_lanes(false, 1), vec![Lane { first_channel: 0, channels: 1 }]);
        assert_eq!(plan_lanes(true, 1), vec![Lane { first_channel: 0, channels: 1 }]);
    }

    #[test]
    fn nova_final_carries_channel_index() {
        let json = r#"{
            "type": "Results", "channel_index": [1, 2], "start": 2.0, "duration": 1.0,
            "is_final": true,
            "channel": { "alternatives": [ { "transcript": "Life moves pretty fast.", "words": [] } ] }
        }"#;
        let fin = nova_final(serde_json::from_str(json).unwrap()).unwrap();
        assert_eq!(fin.channel_index, Some((1, 2)));
        assert_eq!(fin.finalized_through_ms, 3000.0);
        assert_eq!(fin.seg.unwrap().transcript, "Life moves pretty fast.");
    }

    #[test]
    fn empty_final_still_reports_progress() {
        // A silent channel produces empty finals; they must advance its watermark.
        let json = r#"{ "type": "Results", "channel_index": [1, 2], "start": 4.0, "duration": 2.0,
            "is_final": true, "channel": { "alternatives": [ { "transcript": "", "words": [] } ] } }"#;
        let fin = nova_final(serde_json::from_str(json).unwrap()).unwrap();
        assert!(fin.seg.is_none());
        assert_eq!(fin.finalized_through_ms, 6000.0);
    }

    /// Finding 6: on a multichannel socket a missing or invalid channel_index
    /// is unlabelled, never defaulted to channel 0 (= Mic-In).
    #[test]
    fn stereo_result_without_valid_channel_index_is_unlabelled() {
        let stereo = Lane { first_channel: 0, channels: 2 };
        assert_eq!(resolve_channel(stereo, Some((1, 2))), Some(1));
        assert_eq!(resolve_channel(stereo, None), None);
        assert_eq!(resolve_channel(stereo, Some((2, 2))), None);
        assert_eq!(resolve_channel(stereo, Some((0, 1))), None);
        // A mono socket is unambiguous whatever it reports.
        assert_eq!(resolve_channel(Lane { first_channel: 1, channels: 1 }, None), Some(1));
        assert_eq!(parse_channel_index(&serde_json::json!("oops")), None);
        assert_eq!(parse_channel_index(&serde_json::json!([1.5, 2])), None);
        assert_eq!(parse_channel_index(&serde_json::json!([1, 2])), Some((1, 2)));
    }

    fn chunk(data: Vec<f32>, sample_rate: u32, layout: AudioLayout) -> AudioChunk {
        AudioChunk {
            data,
            sample_rate,
            timestamp: 0.0,
            chunk_id: 0,
            device_type: DeviceType::Microphone,
            layout,
        }
    }

    #[test]
    fn lane_samples_keeps_channels_apart_at_native_rate() {
        // 16 kHz input needs no resampling, so samples must pass through exactly.
        let stereo = chunk(vec![0.1, -0.5, 0.2, -0.6, 0.3, -0.7], 16_000, AudioLayout::MicSystemStereo);
        assert_eq!(
            lane_samples(&stereo, Lane { first_channel: 0, channels: 2 }).unwrap(),
            vec![0.1, -0.5, 0.2, -0.6, 0.3, -0.7]
        );
        assert_eq!(lane_samples(&stereo, Lane { first_channel: 0, channels: 1 }).unwrap(), vec![0.1, 0.2, 0.3]);
        assert_eq!(lane_samples(&stereo, Lane { first_channel: 1, channels: 1 }).unwrap(), vec![-0.5, -0.6, -0.7]);
    }

    #[test]
    fn lane_samples_resamples_each_channel_separately() {
        // 48 kHz: left is a constant +0.5 DC level, right is silence. If the
        // interleaved buffer were resampled as one signal the right channel
        // would pick up energy from the left.
        let frames = 4800; // 100 ms
        let mut data = Vec::with_capacity(frames * 2);
        for _ in 0..frames {
            data.push(0.5);
            data.push(0.0);
        }
        let out = lane_samples(&chunk(data, 48_000, AudioLayout::MicSystemStereo), Lane { first_channel: 0, channels: 2 }).unwrap();
        let split = deinterleave(&out, 2);
        assert_eq!(split[0].len(), split[1].len());
        let n = split[0].len();
        assert!((1500..=1700).contains(&n), "expected ~1600 frames at 16 kHz, got {}", n);
        assert!(split[1].iter().all(|s| s.abs() < 1e-6), "right channel must stay silent");
        let mid = &split[0][n / 4..3 * n / 4];
        assert!(mid.iter().all(|s| (s - 0.5).abs() < 0.05), "left channel must keep its level");
    }

    #[test]
    fn mono_chunk_passes_through_unchanged() {
        let mono = chunk(vec![0.1, 0.2, 0.3], 16_000, AudioLayout::MicOnly);
        assert_eq!(lane_samples(&mono, Lane { first_channel: 0, channels: 1 }).unwrap(), vec![0.1, 0.2, 0.3]);
    }

    /// Finding 8: a channel that fails to resample drops the chunk instead of
    /// sending 48 kHz samples labelled as 16 kHz next to a resampled channel.
    #[test]
    fn resample_failure_on_one_channel_drops_the_chunk() {
        let stereo = chunk(vec![0.1, 0.2, 0.3, 0.4], 48_000, AudioLayout::MicSystemStereo);
        let fail_right = |samples: &[f32], _: u32, _: u32| -> Result<Vec<f32>> {
            if samples[0] == 0.2 {
                Err(anyhow!("resampler exploded"))
            } else {
                Ok(samples.to_vec())
            }
        };
        assert_eq!(lane_samples_with(&stereo, Lane { first_channel: 0, channels: 2 }, fail_right), None);
        // The mono lane for the healthy channel is unaffected.
        assert_eq!(
            lane_samples_with(&stereo, Lane { first_channel: 0, channels: 1 }, fail_right),
            Some(vec![0.1, 0.3])
        );
    }

    /// Finding 8: channels that resample to different lengths are padded when
    /// the skew is rounding-sized, dropped when it is not.
    #[test]
    fn unequal_resampled_lengths_pad_small_skew_and_drop_large() {
        let stereo = chunk(vec![1.0; 200], 48_000, AudioLayout::MicSystemStereo);
        let lane = Lane { first_channel: 0, channels: 2 };
        let call = std::sync::atomic::AtomicUsize::new(0);
        let skewed = |short_by: usize| {
            call.store(0, Ordering::SeqCst);
            lane_samples_with(&stereo, lane, |_s: &[f32], _: u32, _: u32| {
                let n = if call.fetch_add(1, Ordering::SeqCst) == 0 { 30 } else { 30 - short_by };
                Ok(vec![1.0; n])
            })
        };
        let small = skewed(1).unwrap();
        assert_eq!(small.len(), 60, "1-sample skew is padded to equal frames");
        assert_eq!(small[59], 0.0, "the pad is silence on the short channel");
        assert_eq!(skewed(10), None, "a large skew drops the chunk");
    }

    #[test]
    fn stereo_chunk_duration_counts_frames_not_samples() {
        // 1600 stereo frames at 16 kHz = 100 ms, even though data holds 3200 samples.
        let stereo = chunk(vec![0.0; 3200], 16_000, AudioLayout::MicSystemStereo);
        assert_eq!(chunk_duration_ms(&stereo), 100.0);
        let mono = chunk(vec![0.0; 3200], 16_000, AudioLayout::MicOnly);
        assert_eq!(chunk_duration_ms(&mono), 200.0);
    }

    #[test]
    fn stereo_time_map_maps_stream_time_to_recording_time() {
        let entries = vec![
            AudioTimeMapEntry { stream_start_ms: 0.0, stream_end_ms: 100.0, original_start_ms: 5000.0, original_end_ms: 5100.0 },
            AudioTimeMapEntry { stream_start_ms: 100.0, stream_end_ms: 200.0, original_start_ms: 5100.0, original_end_ms: 5200.0 },
        ];
        assert_eq!(map_stream_ms_to_original(&entries, 150.0), 5150.0);
    }

    // ------------------------------------------------------------------
    // Bleed suppression
    // ------------------------------------------------------------------

    fn seg(text: &str, start_ms: f64, end_ms: f64, speaker: Option<&'static str>) -> LabeledSegment {
        LabeledSegment { text: text.to_string(), start_ms, end_ms, confidence: 0.9, speaker }
    }

    #[test]
    fn bleed_dropped_when_speaker_copy_arrives_first() {
        let mut s = BleedSuppressor::default();
        let out = s.push(seg("Thanks for joining, Jake.", 1000.0, 1800.0, Some(SPEAKER_OUT_LABEL)), 0.0);
        assert_eq!(out.len(), 1);
        assert!(s.push(seg("Thanks for joining, Jake.", 1050.0, 1800.0, Some(MIC_IN_LABEL)), 100.0).is_empty());
        assert!(s.drain_ready(20_000.0).is_empty());
        assert!(s.flush().is_empty());
    }

    #[test]
    fn bleed_dropped_when_mic_copy_arrives_first() {
        let mut s = BleedSuppressor::default();
        assert!(s.push(seg("Bye bye.", 3000.0, 3500.0, Some(MIC_IN_LABEL)), 0.0).is_empty());
        let out = s.push(seg("Bye bye.", 3000.0, 3600.0, Some(SPEAKER_OUT_LABEL)), 500.0);
        assert_eq!(out, vec![seg("Bye bye.", 3000.0, 3600.0, Some(SPEAKER_OUT_LABEL))]);
        assert!(s.drain_ready(20_000.0).is_empty());
    }

    #[test]
    fn partial_copy_inside_longer_speaker_segment_is_bleed() {
        let mic = seg("I can work with whomever, I just need to know", 1000.0, 3000.0, Some(MIC_IN_LABEL));
        let spk = seg(
            "Yeah. Like I said, I can work with whomever. I just need to know who.",
            900.0,
            4000.0,
            Some(SPEAKER_OUT_LABEL),
        );
        assert!(is_bleed(&mic, &spk));
    }

    #[test]
    fn genuine_mic_speech_released_once_speaker_channel_passes_it() {
        let mut s = BleedSuppressor::default();
        assert!(s.push(seg("Sounds good, let's ship it.", 1500.0, 2500.0, Some(MIC_IN_LABEL)), 0.0).is_empty());
        // Speaker-Out finalized only up to the mic segment's end: keep holding.
        assert!(s.speaker_progress(2500.0, 100.0).is_empty());
        // Past the end plus margin: release.
        let out = s.speaker_progress(2500.0 + BLEED_WATERMARK_MARGIN_MS, 200.0);
        assert_eq!(out, vec![seg("Sounds good, let's ship it.", 1500.0, 2500.0, Some(MIC_IN_LABEL))]);
    }

    #[test]
    fn speaker_final_advances_the_watermark() {
        let mut s = BleedSuppressor::default();
        s.push(seg("Different words entirely here.", 1000.0, 2000.0, Some(MIC_IN_LABEL)), 0.0);
        let out = s.push(seg("Life moves pretty fast.", 1000.0, 4000.0, Some(SPEAKER_OUT_LABEL)), 100.0);
        assert_eq!(out.len(), 2, "speaker line plus the released mic line");
    }

    #[test]
    fn stalled_speaker_channel_releases_mic_at_the_cap() {
        let mut s = BleedSuppressor::default();
        s.push(seg("Anyone there?", 0.0, 800.0, Some(MIC_IN_LABEL)), 0.0);
        assert!(s.drain_ready(BLEED_MAX_HOLD_MS - 1.0).is_empty());
        assert_eq!(s.drain_ready(BLEED_MAX_HOLD_MS).len(), 1);
    }

    #[test]
    fn flux_progress_holds_at_open_turn_start() {
        let mk = |event: &str, transcript: &str, start: f64, end: f64| FluxTurnInfo {
            msg_type: Some("TurnInfo".to_string()),
            event: Some(event.to_string()),
            transcript: transcript.to_string(),
            audio_window_start: start,
            audio_window_end: end,
            words: vec![],
            description: None,
            message: None,
        };
        let mut p = FluxProgress::default();
        assert_eq!(p.observe(&mk("Update", "", 0.0, 1.0)), 1000.0, "silence: final through window end");
        assert_eq!(p.observe(&mk("StartOfTurn", "Life", 1.0, 1.5)), 1000.0);
        assert_eq!(p.observe(&mk("Update", "Life moves", 1.0, 3.0)), 1000.0, "open turn: pending from its start");
        assert_eq!(p.observe(&mk("EndOfTurn", "Life moves pretty fast.", 1.0, 4.0)), 4000.0);
    }

    #[test]
    fn same_words_far_apart_in_time_are_not_bleed() {
        let mic = seg("Thanks for joining, Jake.", 60_000.0, 61_000.0, Some(MIC_IN_LABEL));
        let spk = seg("Thanks for joining, Jake.", 1000.0, 2000.0, Some(SPEAKER_OUT_LABEL));
        assert!(!is_bleed(&mic, &spk));
    }

    #[test]
    fn short_word_inside_long_speaker_segment_is_not_bleed() {
        let mic = seg("Yeah.", 1000.0, 1300.0, Some(MIC_IN_LABEL));
        let spk = seg("Yeah, so the plan for the demo is next week.", 900.0, 4000.0, Some(SPEAKER_OUT_LABEL));
        assert!(!is_bleed(&mic, &spk));
    }

    #[test]
    fn unlabeled_segments_pass_straight_through() {
        let mut s = BleedSuppressor::default();
        let out = s.push(seg("Mixed audio.", 0.0, 500.0, None), 0.0);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn flush_releases_held_mic_segments() {
        let mut s = BleedSuppressor::default();
        s.push(seg("Last words.", 0.0, 500.0, Some(MIC_IN_LABEL)), 0.0);
        assert_eq!(s.flush().len(), 1);
    }

    /// Finding 1: a Mic-In copy must not be released just because a fixed hold
    /// elapsed; the Speaker-Out final that contains it can arrive later.
    #[test]
    fn mic_copy_survives_until_late_longer_speaker_final() {
        let mut s = BleedSuppressor::default();
        assert!(s
            .push(seg("Thanks, Jake. See you then.", 10_000.0, 11_500.0, Some(MIC_IN_LABEL)), 0.0)
            .is_empty());
        // The old fixed 2.5s hold released the copy here.
        assert!(
            s.drain_ready(3_000.0).is_empty(),
            "mic copy released before the speaker lane finalized past it"
        );
        let out = s.push(
            seg("Alright. Thanks, Jake. See you then, bye.", 9_000.0, 12_500.0, Some(SPEAKER_OUT_LABEL)),
            4_000.0,
        );
        assert_eq!(out.len(), 1, "only the Speaker-Out line should be emitted");
        assert_eq!(out[0].speaker, Some(SPEAKER_OUT_LABEL));
        assert!(s.flush().is_empty(), "the held mic copy must have been dropped");
    }

    /// Finding 3: a genuine short reply comes after the far side, not on top of it.
    #[test]
    fn short_reply_after_the_far_side_is_not_bleed() {
        let spk = seg("Okay.", 5_000.0, 5_400.0, Some(SPEAKER_OUT_LABEL));
        let mic = seg("Okay.", 5_800.0, 6_200.0, Some(MIC_IN_LABEL));
        assert!(!is_bleed(&mic, &spk));
    }

    /// Finding 3 (guard): acoustic bleed of a short word is near-instant.
    #[test]
    fn short_simultaneous_copy_is_still_bleed() {
        let spk = seg("Okay.", 5_000.0, 5_400.0, Some(SPEAKER_OUT_LABEL));
        let mic = seg("Okay.", 5_050.0, 5_400.0, Some(MIC_IN_LABEL));
        assert!(is_bleed(&mic, &spk));
    }

    /// Finding 6: an unexpected `channel_index` shape must not reject the whole
    /// Results message (the transcript would be lost).
    #[test]
    fn results_with_malformed_channel_index_still_parse() {
        for bad in [r#""oops""#, "null", r#"[1.5, "x"]"#, "{}"] {
            let json = format!(
                r#"{{ "type": "Results", "channel_index": {}, "start": 0.0, "duration": 1.0,
                     "is_final": true,
                     "channel": {{ "alternatives": [ {{ "transcript": "Still here.", "words": [] }} ] }} }}"#,
                bad
            );
            let msg: RealtimeMessage = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("channel_index {} rejected the message: {}", bad, e));
            assert!(nova_result_to_final(msg).is_some());
        }
    }

    // ------------------------------------------------------------------
    // Failure paths (finding 5), against a local WebSocket server
    // ------------------------------------------------------------------

    /// Serve `script` to each accepted socket in turn: Ok(messages) sends those
    /// text frames then keeps reading; Err(()) rejects the handshake.
    async fn mock_deepgram(
        script: Vec<std::result::Result<Vec<String>, ()>>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("ws://{}/listen", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            for step in script {
                let Ok((stream, _)) = listener.accept().await else { return };
                match step {
                    Err(()) => {
                        // Not a WebSocket upgrade: the client's handshake fails.
                        use tokio::io::AsyncWriteExt;
                        let mut stream = stream;
                        let _ = stream
                            .write_all(b"HTTP/1.1 400 Bad Request\r\ncontent-length: 0\r\n\r\n")
                            .await;
                    }
                    Ok(messages) => {
                        tokio::spawn(async move {
                            let Ok(ws) = tokio_tungstenite::accept_async(stream).await else { return };
                            let (mut tx, mut rx) = ws.split();
                            for m in messages {
                                // "__sleep:<ms>" pauses the script instead of sending.
                                if let Some(ms) = m.strip_prefix("__sleep:") {
                                    tokio::time::sleep(Duration::from_millis(ms.parse().unwrap())).await;
                                    continue;
                                }
                                tokio::time::sleep(Duration::from_millis(50)).await;
                                if tx.send(Message::Text(m)).await.is_err() {
                                    return;
                                }
                            }
                            while let Some(Ok(_)) = rx.next().await {}
                        });
                    }
                }
            }
        });
        (base, handle)
    }

    fn feed(layout: AudioLayout) -> mpsc::UnboundedReceiver<AudioChunk> {
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            for i in 0..200u64 {
                let data = vec![0.0; 1600 * layout.channels() as usize];
                let chunk = AudioChunk {
                    data,
                    sample_rate: 16_000,
                    timestamp: i as f64 * 0.1,
                    chunk_id: i,
                    device_type: DeviceType::Microphone,
                    layout,
                };
                if tx.send(chunk).is_err() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
        rx
    }

    fn results(channel: usize, start: f64, text: &str) -> String {
        serde_json::json!({
            "type": "Results", "channel_index": [channel, 2], "start": start, "duration": 1.0,
            "is_final": true, "channel": { "alternatives": [ { "transcript": text, "words": [] } ] }
        })
        .to_string()
    }

    type Emitted = Arc<std::sync::Mutex<Vec<LabeledSegment>>>;

    async fn run_against(
        endpoints: RealtimeEndpoints,
        model: String,
        layout: AudioLayout,
        emitted: Emitted,
    ) -> (std::result::Result<(), String>, Emitted) {
        run_against_slow(endpoints, model, layout, emitted, Duration::ZERO).await
    }

    /// `emit_delay` makes every emit slow, so an emit racing the return of
    /// `run_realtime_core` lands visibly after it.
    async fn run_against_slow(
        endpoints: RealtimeEndpoints,
        model: String,
        layout: AudioLayout,
        emitted: Emitted,
        emit_delay: Duration,
    ) -> (std::result::Result<(), String>, Emitted) {
        let sink = emitted.clone();
        let options = DeepgramOptions { model, ..DeepgramOptions::default() };
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            run_realtime_core(endpoints, "k".to_string(), options, None, feed(layout), move |s| {
                std::thread::sleep(emit_delay);
                sink.lock().unwrap().push(s)
            }),
        )
        .await
        .expect("core must return promptly on error, not wait out the 30s reader deadline");
        (result, emitted)
    }

    /// A reader error ends the session with nothing emitted afterwards: the
    /// held Mic-In line is released before return, never after.
    // Multi-threaded: on a current-thread runtime the emitter always finishes
    // before the session can resume, which would hide a missing await.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reader_error_stops_every_task_before_returning() {
        let (base, _server) = mock_deepgram(vec![Ok(vec![
            // The mic line ends past the far side's watermark, so it is still
            // held when the Error lands and must be flushed before return.
            results(1, 0.0, "Far side line here."),
            results(0, 10.0, "Held mic line here."),
            serde_json::json!({ "type": "Error", "description": "boom" }).to_string(),
        ])])
        .await;
        let endpoints = RealtimeEndpoints { nova: base.clone(), flux: base };
        let (result, emitted) = run_against_slow(
            endpoints,
            "nova-3".into(),
            AudioLayout::MicSystemStereo,
            Emitted::default(),
            Duration::from_millis(300),
        )
        .await;
        assert!(result.unwrap_err().contains("boom"));
        let at_return = emitted.lock().unwrap().len();
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(emitted.lock().unwrap().len(), at_return, "emit after return");
        let texts: Vec<String> = emitted.lock().unwrap().iter().map(|s| s.text.clone()).collect();
        assert!(texts.contains(&"Held mic line here.".to_string()), "held line flushed before return: {:?}", texts);
    }

    /// A Flux lane that fails to connect leaves no task running.
    #[tokio::test]
    async fn flux_second_lane_connect_failure_spawns_nothing() {
        let (base, _server) = mock_deepgram(vec![Ok(vec![]), Err(())]).await;
        let endpoints = RealtimeEndpoints { nova: base.clone(), flux: base };
        let (result, emitted) = run_against(endpoints, "flux-general-en".into(), AudioLayout::MicSystemStereo, Emitted::default()).await;
        assert!(result.unwrap_err().contains("Failed to connect"));
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(emitted.lock().unwrap().is_empty());
    }

    /// Finding 2: a mic-only stream uses one mono socket and emits Mic-In lines
    /// immediately (no bleed hold: there is no far side to copy).
    #[tokio::test]
    async fn mic_only_stream_is_mono_and_unheld() {
        let mono_result = serde_json::json!({
            "type": "Results", "channel_index": [0, 1], "start": 0.0, "duration": 1.0,
            "is_final": true, "channel": { "alternatives": [ { "transcript": "Just me talking.", "words": [] } ] }
        })
        .to_string();
        let (base, _server) = mock_deepgram(vec![Ok(vec![
            mono_result,
            "__sleep:2000".to_string(),
            serde_json::json!({ "type": "Error", "description": "end" }).to_string(),
        ])])
        .await;
        let endpoints = RealtimeEndpoints { nova: base.clone(), flux: base };
        let emitted = Emitted::default();
        let session = tokio::spawn(run_against(endpoints, "nova-3".into(), AudioLayout::MicOnly, emitted.clone()));
        // The Error (which would flush any held line) is 2s away; an unheld
        // line is already out.
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let early = emitted.lock().unwrap().clone();
        assert_eq!(early.len(), 1, "mic-only line was held");
        assert_eq!(early[0].speaker, Some(MIC_IN_LABEL));
        let _ = session.await.unwrap();
    }

    /// Live wire test against api.deepgram.com through the production realtime
    /// core. Needs ~/.config/meetily-dgkey and MEETILY_STEREO_PCM pointing at
    /// 16 kHz s16le stereo (left = one talker, right = another).
    /// Run: cargo test --lib live_stereo_realtime -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_stereo_realtime_labels_each_channel() {
        let (key, samples) = live_inputs();
        for model in ["nova-3", "flux-general-en"] {
            let segs = live_run(&key, model, &samples, AudioLayout::MicSystemStereo).await;
            let mic: String = segs.iter().filter(|s| s.speaker == Some(MIC_IN_LABEL)).map(|s| s.text.to_lowercase()).collect();
            let spk: String = segs.iter().filter(|s| s.speaker == Some(SPEAKER_OUT_LABEL)).map(|s| s.text.to_lowercase()).collect();
            assert!(mic.contains("spacewalk") || mic.contains("space walk"), "{}: mic channel missing spacewalk: {}", model, mic);
            assert!(spk.contains("life moves pretty fast"), "{}: speaker channel missing Bueller: {}", model, spk);
            assert!(!mic.contains("life moves"), "{}: Bueller leaked into Mic-In", model);
            assert!(!spk.contains("spacewalk"), "{}: spacewalk leaked into Speaker-Out", model);
            assert!(segs.iter().all(|s| s.speaker.is_some()), "{}: unlabelled line on a stereo stream", model);
        }
    }

    /// Live mic-only run: the left channel alone as a mono MicOnly stream.
    #[tokio::test]
    #[ignore]
    async fn live_mic_only_realtime_is_mono_mic_in() {
        let (key, samples) = live_inputs();
        let left: Vec<f32> = samples.chunks_exact(2).map(|f| f[0]).collect();
        for model in ["nova-3", "flux-general-en"] {
            let segs = live_run(&key, model, &left, AudioLayout::MicOnly).await;
            assert!(!segs.is_empty());
            assert!(segs.iter().all(|s| s.speaker == Some(MIC_IN_LABEL)), "{}: {:?}", model, segs);
            let text: String = segs.iter().map(|s| s.text.to_lowercase()).collect();
            assert!(text.contains("spacewalk") || text.contains("space walk"), "{}: {}", model, text);
        }
    }

    fn live_inputs() -> (String, Vec<f32>) {
        let key = std::fs::read_to_string(
            std::path::Path::new(&std::env::var("HOME").unwrap()).join(".config/meetily-dgkey"),
        )
        .unwrap()
        .trim()
        .to_string();
        let pcm = std::fs::read(std::env::var("MEETILY_STEREO_PCM").unwrap()).unwrap();
        let samples = pcm
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
            .collect();
        (key, samples)
    }

    async fn live_run(key: &str, model: &str, samples: &[f32], layout: AudioLayout) -> Vec<LabeledSegment> {
        let (tx, rx) = mpsc::unbounded_channel();
        let collected: Emitted = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = collected.clone();
        let started = std::time::Instant::now();
        let options = DeepgramOptions { model: model.to_string(), ..DeepgramOptions::default() };
        let session = tokio::spawn(run_realtime_core(
            RealtimeEndpoints::deepgram(),
            key.to_string(),
            options,
            None,
            rx,
            move |s| {
                println!(
                    "[{} {:?}] emitted at {:>5.1}s  audio {:>5.1}s  {:<11} {}",
                    "live",
                    layout,
                    started.elapsed().as_secs_f64(),
                    s.start_ms / 1000.0,
                    s.speaker.unwrap_or("-"),
                    s.text
                );
                sink.lock().unwrap().push(s)
            },
        ));
        // 100 ms frames, paced near realtime (faster overruns the socket).
        let per_chunk = 1600 * layout.channels() as usize;
        for (i, block) in samples.chunks(per_chunk).enumerate() {
            tx.send(AudioChunk {
                data: block.to_vec(),
                sample_rate: 16_000,
                timestamp: i as f64 * 0.1,
                chunk_id: i as u64,
                device_type: DeviceType::Microphone,
                layout,
            })
            .unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        drop(tx);
        session.await.unwrap().unwrap();
        println!("[{}] ---", model);
        let segs = collected.lock().unwrap().clone();
        segs
    }
}
