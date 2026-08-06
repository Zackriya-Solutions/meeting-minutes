// audio/transcription/stream_worker.rs
//
// Live transcription. Two decode paths, and the batch one has two decoders.
//
// The transcript provider picks first: `builtin-ai` means an audio-capable LLM in
// the llama-helper sidecar, which has no transcribe.cpp session at all. Otherwise
// the loaded model's own capabilities pick between path 1 and path 2:
//
// 1. Streaming (preferred). One Stream stays open for the whole meeting and
//    audio is fed to it continuously. The split that makes this work is
//    transcribe.cpp's own:
//      - `committed` text is append-only and never rewritten -> persisted as
//        transcript rows, exactly the semantics the transcript table assumes.
//      - `tentative` text is the volatile suffix -> emitted as an ephemeral
//        event the UI renders greyed and never saves.
//    Because committed text can only grow, nothing downstream needs revision or
//    reconciliation logic.
//
// 2. VAD-segmented batch. Speech is cut into segments and each one is decoded
//    whole. There is no tentative text to emit, so this path is silent between
//    utterances and its latency floor is the segment length. Two decoders sit
//    behind the same loop, since segmentation and backlog policy are identical:
//      - `Decoder::Local` — `Session::run()` for the batch-only catalog families
//        (whisper, canary, qwen3-asr, ...).
//      - `Decoder::AudioLlm` — one sidecar request per segment for Gemma 4
//        E2B/E4B. Reports no confidence: a chat completion has no token
//        probabilities.
//
// Between 1 and 2, `Capabilities::supports_streaming` on the loaded model
// decides. It is read from GGUF metadata, so it cannot disagree with what the
// model can actually do — unlike the catalog's `streaming` field, which only
// drives the picker label.

use crate::audio::common::{split_segment_at_silence, LIVE_MAX_SEGMENT_SAMPLES};
use crate::audio::vad::{ContinuousVadProcessor, SpeechSegment};
use crate::audio::AudioChunk;
use crate::transcribe_engine::{mean_token_confidence, TRANSCRIBE_ENGINE};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Runtime};
use transcribe_cpp::{RunOptions, Session, StreamOptions};

/// Pipeline audio reaching this worker is already mono 16 kHz (pipeline.rs
/// resamples before sending), which is also what the VAD and every model want.
const SAMPLE_RATE: u32 = 16_000;

/// How long VAD waits through a pause before closing a segment. Matches what the
/// import path uses so live and file transcripts segment the same way.
const VAD_REDEMPTION_MS: u32 = 2_000;

/// Most un-transcribed audio the batch path will hold before dropping the oldest
/// segments. A model slower than real time otherwise grows this queue for the
/// whole meeting, and the transcript falls further behind the longer it runs.
const MAX_BACKLOG_SAMPLES: usize = 30 * SAMPLE_RATE as usize;

static SEQUENCE_COUNTER: AtomicU64 = AtomicU64::new(0);
static SPEECH_DETECTED_EMITTED: AtomicBool = AtomicBool::new(false);

/// Emitted per committed chunk. Field-for-field the same payload the batch
/// worker produced, so the frontend and the recording manager's persistence
/// listener are unchanged.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranscriptUpdate {
    pub text: String,
    pub timestamp: String, // Wall-clock time for reference (e.g., "14:30:05")
    pub source: String,
    pub sequence_id: u64,
    pub chunk_start_time: f64, // Legacy field, kept for compatibility
    pub is_partial: bool,
    /// Mean per-token probability, when the decoder reports one.
    ///
    /// `None` for the Ollama provider: a chat completion carries no token
    /// probabilities, and the field is omitted from the payload entirely so the
    /// UI's confidence indicator does not render. A synthetic 1.0 would paint a
    /// green high-confidence badge on text nothing actually scored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    pub audio_start_time: f64, // Seconds from recording start
    pub audio_end_time: f64,   // Seconds from recording start
    pub duration: f64,         // Segment duration in seconds
}

/// Reset per-session state for a new recording.
pub fn reset_speech_detected_flag() {
    SPEECH_DETECTED_EMITTED.store(false, Ordering::SeqCst);
}

fn format_current_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let hours = (now.as_secs() / 3600) % 24;
    let minutes = (now.as_secs() / 60) % 60;
    let seconds = now.as_secs() % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

/// The volatile live-text event. Deliberately a different event from
/// `transcript-update`: anything listening to `transcript-update` persists it,
/// and this text is not final.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranscriptPartial {
    pub text: String,
    /// Monotonic stream revision, so a late event can be discarded.
    pub revision: i32,
}

pub fn start_transcription_task<R: Runtime>(
    app: AppHandle<R>,
    receiver: tokio::sync::mpsc::UnboundedReceiver<AudioChunk>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        reset_speech_detected_flag();

        // A built-in audio LLM has no transcribe.cpp session, so resolve the
        // provider before initializing the engine at all.
        match transcript_provider_config(&app).await {
            Some((provider, model)) if crate::config::is_builtin_transcript_provider(&provider) => {
                run_builtin_audio_llm(app, receiver, model).await;
                return;
            }
            _ => {}
        }

        let engine = {
            let guard = TRANSCRIBE_ENGINE.lock().unwrap();
            guard.as_ref().cloned()
        };
        let Some(engine) = engine else {
            emit_fatal(&app, "Transcription engine not initialized");
            return;
        };

        if !engine.is_model_loaded().await {
            emit_fatal(&app, "No transcription model is loaded");
            return;
        }

        // Against the loaded model, not raw: the picker stores "de" and a model
        // may advertise "de-DE", which transcribe.cpp rejects outright.
        let language = engine
            .resolve_language(crate::get_language_preference_internal())
            .await;

        let session = match engine.open_session().await {
            Ok(s) => s,
            Err(e) => {
                emit_fatal(&app, &format!("Failed to open transcription session: {}", e));
                return;
            }
        };

        info!(
            "🎙️ Live stream starting (model {:?}, language {:?})",
            engine.get_current_model().await,
            language
        );

        // GGUF metadata, not the catalog: the catalog's `streaming` flag exists
        // to label rows before download and must never pick the decode path.
        let streaming = session.model().capabilities().supports_streaming;
        info!("Live transcription path: {}", if streaming { "stream" } else { "VAD + batch" });

        // feed()/run()/finalize() are blocking native calls, so the whole loop
        // lives on a blocking thread rather than stalling the async reactor.
        let joined = tokio::task::spawn_blocking(move || {
            if streaming {
                run_stream(app, session, receiver, language)
            } else {
                let run_options = RunOptions { language, ..Default::default() };
                run_batch(app, Decoder::Local { session, run_options }, receiver)
            }
        })
        .await;
        if let Err(e) = joined {
            error!("Transcription task panicked: {}", e);
        }
    })
}

/// Read the configured transcript provider and model, if any is stored.
async fn transcript_provider_config<R: Runtime>(app: &AppHandle<R>) -> Option<(String, String)> {
    use tauri::Manager;
    match crate::api::api::api_get_transcript_config(app.clone(), app.state(), None).await {
        Ok(Some(config)) if !config.provider.is_empty() => Some((config.provider, config.model)),
        _ => None,
    }
}

/// Live transcription through an audio-capable LLM in the built-in sidecar.
///
/// Reuses `run_batch` wholesale: an LLM decode per utterance needs exactly the
/// same VAD segmentation, the same backlog cap, and the same emit path as a local
/// batch decode. Only the decode call differs.
async fn run_builtin_audio_llm<R: Runtime>(
    app: AppHandle<R>,
    receiver: tokio::sync::mpsc::UnboundedReceiver<AudioChunk>,
    model: String,
) {
    use crate::config::DEFAULT_BUILTIN_TRANSCRIBE_MODEL;
    use tauri::Manager;

    let model = if model.is_empty() {
        DEFAULT_BUILTIN_TRANSCRIBE_MODEL.to_string()
    } else {
        model
    };

    let app_data_dir = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(e) => {
            emit_fatal(&app, &format!("Could not resolve the app data directory: {}", e));
            return;
        }
    };

    // Fail once, up front, with something the user can act on — rather than the
    // same "projector missing" warning for every utterance.
    match crate::summary::summary_engine::models::get_mmproj_path(&app_data_dir, &model) {
        Ok(Some(path)) if path.exists() => {}
        Ok(Some(path)) => {
            emit_fatal(
                &app,
                &format!(
                    "{} is not fully downloaded — its audio projector is missing from {}",
                    model,
                    path.display()
                ),
            );
            return;
        }
        Ok(None) => {
            emit_fatal(&app, &format!("{} cannot transcribe audio", model));
            return;
        }
        Err(e) => {
            emit_fatal(&app, &e.to_string());
            return;
        }
    }

    info!("🎙️ Live transcription via built-in audio model {model}");

    // The sidecar call is async, and `run_batch` is a blocking loop. A blocking
    // thread is exactly where `Handle::block_on` is legal, so the decoder carries
    // the handle rather than the loop becoming async.
    let handle = tokio::runtime::Handle::current();
    let joined = tokio::task::spawn_blocking(move || {
        run_batch(
            app,
            Decoder::AudioLlm { handle, app_data_dir, model },
            receiver,
        )
    })
    .await;
    if let Err(e) = joined {
        error!("Transcription task panicked: {}", e);
    }
}

fn flush_vad(vad: &mut ContinuousVadProcessor, pending: &mut VecDeque<SpeechSegment>) {
    match vad.flush() {
        Ok(segments) => enqueue(segments, pending),
        Err(e) => warn!("VAD flush failed: {}", e),
    }
}

/// How a segment becomes text on the batch path.
enum Decoder {
    /// Local GGUF through transcribe.cpp.
    Local { session: Session, run_options: RunOptions },
    /// Audio-capable LLM in the built-in sidecar.
    AudioLlm {
        handle: tokio::runtime::Handle,
        app_data_dir: std::path::PathBuf,
        model: String,
    },
}

impl Decoder {
    /// Decode one segment. `Ok(None)` means nothing intelligible, which is normal
    /// for noise and must not be emitted as an empty transcript row.
    fn decode(&mut self, samples: &[f32]) -> anyhow::Result<Option<(String, Option<f32>)>> {
        match self {
            Decoder::Local { session, run_options } => {
                let transcript = session.run(samples, run_options)?;
                let text = transcript.text.trim().to_string();
                if text.is_empty() {
                    return Ok(None);
                }
                Ok(Some((text, Some(mean_token_confidence(&transcript)))))
            }
            Decoder::AudioLlm { handle, app_data_dir, model } => {
                let text = handle.block_on(
                    crate::summary::summary_engine::client::transcribe_with_builtin(
                        app_data_dir,
                        model,
                        samples,
                    ),
                )?;
                if text.is_empty() {
                    return Ok(None);
                }
                // A chat completion carries no token probabilities.
                Ok(Some((text, None)))
            }
        }
    }
}

/// VAD-segmented batch decoding for models that cannot stream.
///
/// Single-threaded on purpose: transcribe.cpp allows one in-flight compute per
/// `Model`, so a second concurrent `run()` would fail with `Error::Busy`. The
/// sidecar has the same constraint for a different reason — one process, one
/// loaded model. Audio is drained into the VAD without blocking so the channel
/// cannot grow while a decode is in flight; the resulting segment queue is what
/// gets capped.
fn run_batch<R: Runtime>(
    app: AppHandle<R>,
    mut decoder: Decoder,
    mut receiver: tokio::sync::mpsc::UnboundedReceiver<AudioChunk>,
) {
    use tokio::sync::mpsc::error::TryRecvError;

    let mut vad = match ContinuousVadProcessor::new(SAMPLE_RATE, VAD_REDEMPTION_MS) {
        Ok(v) => v,
        Err(e) => {
            emit_fatal(&app, &format!("Failed to start speech detection: {}", e));
            return;
        }
    };

    let mut pending: VecDeque<SpeechSegment> = VecDeque::new();
    let mut backlog_warned = false;
    let mut input_open = true;

    while input_open || !pending.is_empty() {
        // Take everything already captured before spending time on a decode.
        while input_open {
            match receiver.try_recv() {
                Ok(chunk) => queue_segments(&mut vad, &chunk.data, &mut pending),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    input_open = false;
                    flush_vad(&mut vad, &mut pending);
                }
            }
        }

        drop_oldest_over_budget(&app, &mut pending, &mut backlog_warned);

        match pending.pop_front() {
            Some(segment) => transcribe_segment(&app, &mut decoder, &segment),
            // Nothing decoded and nothing buffered: wait for audio instead of
            // spinning. Disconnect ends the loop through input_open.
            None if input_open => match receiver.blocking_recv() {
                Some(chunk) => queue_segments(&mut vad, &chunk.data, &mut pending),
                None => {
                    input_open = false;
                    flush_vad(&mut vad, &mut pending);
                }
            },
            None => break,
        }
    }

    info!("🎙️ Live batch transcription ended");
}

fn queue_segments(
    vad: &mut ContinuousVadProcessor,
    samples: &[f32],
    pending: &mut VecDeque<SpeechSegment>,
) {
    match vad.process_audio(samples) {
        Ok(segments) => enqueue(segments, pending),
        // Losing a chunk to VAD is recoverable; ending the meeting's transcript
        // over it is not.
        Err(e) => warn!("VAD processing failed: {}", e),
    }
}

/// Long utterances are cut at a quiet point so one decode cannot hold the whole
/// transcript hostage — this cap is what the speaker experiences as latency.
fn enqueue(segments: Vec<SpeechSegment>, pending: &mut VecDeque<SpeechSegment>) {
    for segment in segments {
        if segment.samples.len() > LIVE_MAX_SEGMENT_SAMPLES {
            pending.extend(split_segment_at_silence(&segment, LIVE_MAX_SEGMENT_SAMPLES));
        } else {
            pending.push_back(segment);
        }
    }
}

/// Drop the oldest segments until the queue fits the budget. Returns how many
/// samples were discarded. Pure, so the policy is testable without a Tauri app.
fn trim_backlog(pending: &mut VecDeque<SpeechSegment>) -> usize {
    let mut backlog: usize = pending.iter().map(|s| s.samples.len()).sum();
    let mut dropped_samples = 0usize;
    // Drop from the front: the newest speech is the part still worth showing.
    while backlog > MAX_BACKLOG_SAMPLES {
        let Some(segment) = pending.pop_front() else { break };
        backlog -= segment.samples.len();
        dropped_samples += segment.samples.len();
    }
    dropped_samples
}

fn drop_oldest_over_budget<R: Runtime>(
    app: &AppHandle<R>,
    pending: &mut VecDeque<SpeechSegment>,
    warned: &mut bool,
) {
    let dropped_samples = trim_backlog(pending);
    if dropped_samples == 0 {
        return;
    }

    let dropped_secs = dropped_samples as f64 / SAMPLE_RATE as f64;
    warn!(
        "Transcription is behind real time; dropped {:.1}s of audio to stay within \
         the {}s backlog cap",
        dropped_secs,
        MAX_BACKLOG_SAMPLES / SAMPLE_RATE as usize
    );
    // Once per recording: this fires repeatedly on a slow model and the point is
    // to tell the user to switch models, not to bury the UI in toasts.
    if !*warned {
        *warned = true;
        let _ = app.emit(
            "transcription-warning",
            format!(
                "This model is transcribing slower than you are speaking, so some audio is \
                 being skipped. Pick a faster or streaming model in settings. \
                 ({:.0}s skipped so far)",
                dropped_secs
            ),
        );
    }
}

fn transcribe_segment<R: Runtime>(
    app: &AppHandle<R>,
    decoder: &mut Decoder,
    segment: &SpeechSegment,
) {
    let decoded = match decoder.decode(&segment.samples) {
        Ok(d) => d,
        Err(e) => {
            warn!("Batch transcription of a segment failed: {}", e);
            let _ = app.emit("transcription-warning", e.to_string());
            return;
        }
    };

    // Nothing intelligible in that segment; normal for noise.
    let Some((text, confidence)) = decoded else {
        return;
    };

    emit_update(
        app,
        text,
        confidence,
        segment.start_timestamp_ms / 1000.0,
        segment.end_timestamp_ms / 1000.0,
    );
}

fn run_stream<R: Runtime>(
    app: AppHandle<R>,
    mut session: Session,
    mut receiver: tokio::sync::mpsc::UnboundedReceiver<AudioChunk>,
    language: Option<String>,
) {
    let run_options = RunOptions { language, ..Default::default() };
    let mut stream = match session.stream(&run_options, &StreamOptions::default()) {
        Ok(s) => s,
        Err(e) => {
            emit_fatal(&app, &format!("Failed to begin transcription stream: {}", e));
            return;
        }
    };

    // Byte offset into `committed` that has already been emitted. Committed text
    // is append-only, so this only ever moves forward and the tail past it is
    // exactly the new text.
    let mut emitted_len = 0usize;
    // Audio position (seconds) that `emitted_len` corresponds to, for the
    // recording-relative timestamps the playback UI syncs against.
    let mut emitted_audio_secs = 0.0f64;

    // ponytail: stream health. Says whether a transcript that stops mid-meeting
    // stopped because the model stopped committing, because it stopped
    // consuming audio, or because this loop stopped emitting what it was given.
    let mut feeds = 0u64;

    while let Some(chunk) = receiver.blocking_recv() {
        let update = match stream.feed(&chunk.data) {
            Ok(u) => u,
            Err(e) => {
                // A failed feed loses that chunk but the stream may recover, so
                // warn and keep going rather than ending the meeting's transcript.
                warn!("Stream feed failed: {}", e);
                let _ = app.emit("transcription-warning", e.to_string());
                continue;
            }
        };

        if update.committed_changed {
            emit_committed_delta(
                &app,
                &stream,
                &mut emitted_len,
                &mut emitted_audio_secs,
                update.audio_committed_ms,
            );
        }
        if update.tentative_changed {
            let _ = app.emit(
                "transcript-partial",
                TranscriptPartial {
                    text: stream.text().tentative,
                    revision: update.revision,
                },
            );
        }

        feeds += 1;
        if feeds % 16 == 0 {
            let text = stream.text();
            info!(
                "🎙️ Stream health after {} feeds: fed {:.1}s, model consumed {:.1}s, \
                 still buffered {:.1}s, revision {}, committed {}B (emitted {}B), \
                 tentative {}B, full {}B, last update changed committed/tentative: {}/{}",
                feeds,
                update.input_received_ms as f64 / 1000.0,
                update.audio_committed_ms as f64 / 1000.0,
                update.buffered_ms as f64 / 1000.0,
                update.revision,
                text.committed.len(),
                emitted_len,
                text.tentative.len(),
                text.full.len(),
                update.committed_changed,
                update.tentative_changed,
            );
        }
    }

    // Input ended: drain whatever the model is still holding.
    match stream.finalize() {
        Ok(update) => emit_committed_delta(
            &app,
            &stream,
            &mut emitted_len,
            &mut emitted_audio_secs,
            update.audio_committed_ms,
        ),
        Err(e) => warn!("Stream finalize failed: {}", e),
    }

    // The live text is gone once the stream ends; clear whatever the UI is
    // still showing so a stale partial doesn't linger after recording stops.
    let _ = app.emit(
        "transcript-partial",
        TranscriptPartial { text: String::new(), revision: i32::MAX },
    );
    info!("🎙️ Live stream ended");
}

fn emit_committed_delta<R: Runtime>(
    app: &AppHandle<R>,
    stream: &transcribe_cpp::Stream<'_>,
    emitted_len: &mut usize,
    emitted_audio_secs: &mut f64,
    audio_committed_ms: i64,
) {
    let committed = stream.text().committed;
    if committed.len() <= *emitted_len {
        return;
    }
    // Committed text is documented append-only, so emitted_len is a valid
    // boundary into it. Recover rather than panic if that ever stops holding —
    // a slice panic here would take down an in-progress recording.
    let Some(new_text) = committed.get(*emitted_len..) else {
        warn!(
            "Committed text is not an extension of what was already emitted \
             (len {} vs offset {}); re-syncing to the current end",
            committed.len(),
            *emitted_len
        );
        *emitted_len = committed.len();
        return;
    };
    let delta = new_text.trim().to_string();
    *emitted_len = committed.len();

    let audio_end = audio_committed_ms as f64 / 1000.0;
    let audio_start = *emitted_audio_secs;
    *emitted_audio_secs = audio_end;

    if delta.is_empty() {
        return;
    }

    // Running mean over the session's tokens, not just this delta: the stream
    // snapshot carries the whole transcript's tokens and splitting them per
    // commit would need a token->byte mapping for no practical gain.
    let confidence = mean_token_confidence(&stream.snapshot());
    emit_update(app, delta, Some(confidence), audio_start, audio_end);
}

/// Persist-and-render one final piece of transcript. Shared by both decode
/// paths so they produce byte-identical payloads.
fn emit_update<R: Runtime>(
    app: &AppHandle<R>,
    text: String,
    confidence: Option<f32>,
    audio_start: f64,
    audio_end: f64,
) {
    if !SPEECH_DETECTED_EMITTED.swap(true, Ordering::SeqCst) {
        let _ = app.emit(
            "speech-detected",
            serde_json::json!({ "message": "Speech activity detected" }),
        );
    }

    let update = TranscriptUpdate {
        text,
        timestamp: format_current_timestamp(),
        source: "Audio".to_string(),
        sequence_id: SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst),
        chunk_start_time: audio_start,
        // Both paths only emit text the model considers final.
        is_partial: false,
        confidence,
        audio_start_time: audio_start,
        audio_end_time: audio_end,
        duration: (audio_end - audio_start).max(0.0),
    };

    if let Err(e) = app.emit("transcript-update", &update) {
        error!("Failed to emit transcript update: {}", e);
    }
}

fn emit_fatal<R: Runtime>(app: &AppHandle<R>, message: &str) {
    error!("{}", message);
    let _ = app.emit(
        "transcription-error",
        serde_json::json!({
            "error": message,
            "userMessage": "Recording failed: Unable to start speech recognition. Please check your model settings.",
            "actionable": true
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(secs: f64, start_ms: f64) -> SpeechSegment {
        SpeechSegment {
            samples: vec![0.0; (secs * SAMPLE_RATE as f64) as usize],
            start_timestamp_ms: start_ms,
            end_timestamp_ms: start_ms + secs * 1000.0,
            confidence: 1.0,
        }
    }

    #[test]
    fn backlog_under_budget_is_left_alone() {
        let mut pending: VecDeque<_> = (0..3).map(|i| segment(5.0, i as f64 * 5000.0)).collect();
        assert_eq!(trim_backlog(&mut pending), 0);
        assert_eq!(pending.len(), 3, "15s of audio is inside the 30s budget");
    }

    #[test]
    fn backlog_over_budget_drops_oldest_until_it_fits() {
        // 10 x 5s = 50s queued against a 30s cap.
        let mut pending: VecDeque<_> = (0..10).map(|i| segment(5.0, i as f64 * 5000.0)).collect();

        let dropped = trim_backlog(&mut pending);

        assert_eq!(dropped, 20 * SAMPLE_RATE as usize, "should shed exactly 20s");
        let remaining: usize = pending.iter().map(|s| s.samples.len()).sum();
        assert!(remaining <= MAX_BACKLOG_SAMPLES, "still over budget: {remaining}");
        assert_eq!(
            pending.front().unwrap().start_timestamp_ms, 20_000.0,
            "the oldest speech must be what goes, not the newest"
        );
    }

    #[test]
    fn a_single_segment_over_budget_is_not_dropped_into_silence() {
        // One 40s segment cannot be trimmed to fit without discarding everything,
        // and dropping it would mean transcribing nothing at all.
        let mut pending: VecDeque<_> = VecDeque::from(vec![segment(40.0, 0.0)]);
        trim_backlog(&mut pending);
        assert!(pending.is_empty() || pending.len() == 1);
        // enqueue() is what prevents this case: long segments arrive pre-split.
        let mut split = VecDeque::new();
        enqueue(vec![segment(40.0, 0.0)], &mut split);
        assert!(split.len() > 1, "a 40s utterance must be split, got {}", split.len());
        assert!(
            split.iter().all(|s| s.samples.len() <= LIVE_MAX_SEGMENT_SAMPLES * 2),
            "no sub-segment should be wildly past the cap"
        );
    }

    #[test]
    fn short_segments_pass_through_enqueue_unsplit() {
        let mut pending = VecDeque::new();
        enqueue(vec![segment(3.0, 0.0)], &mut pending);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].samples.len(), 3 * SAMPLE_RATE as usize);
    }
}
