// audio/transcription/worker.rs
//
// Parallel transcription worker pool and chunk processing logic.

use super::engine::TranscriptionEngine;
use super::provider::TranscriptionError;
use crate::audio::AudioChunk;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Runtime};

// Sequence counter for transcript updates
static SEQUENCE_COUNTER: AtomicU64 = AtomicU64::new(0);

// Speech detection flag - reset per recording session
static SPEECH_DETECTED_EMITTED: AtomicBool = AtomicBool::new(false);

/// Reset the speech detected flag for a new recording session
pub fn reset_speech_detected_flag() {
    SPEECH_DETECTED_EMITTED.store(false, Ordering::SeqCst);
}

/// Returns true if the transcript text is non-trivial and should be emitted.
/// Filters empty/whitespace-only text; no confidence gating is applied.
fn should_emit_transcript(text: &str) -> bool {
    !text.trim().is_empty()
}

fn should_log_transcript_snippet(accepted_count: u64) -> bool {
    accepted_count <= 5 || accepted_count % 25 == 0
}

#[derive(Default)]
struct WorkerLogStats {
    queued: u64,
    completed: u64,
    emitted: u64,
    empty: u64,
    too_short: u64,
    failed: u64,
    max_queue_depth: u64,
    total_processing_duration: Duration,
    confidence_total: f64,
    confidence_count: u64,
}

impl WorkerLogStats {
    fn snapshot_and_reset(&mut self) -> Self {
        std::mem::take(self)
    }
}

fn emit_worker_log_snapshot(stats: WorkerLogStats, final_summary: bool) {
    let confidence_average = if stats.confidence_count == 0 {
        0.0
    } else {
        stats.confidence_total / stats.confidence_count as f64
    };

    if final_summary {
        info!(
            "transcription worker final summary queued={} completed={} emitted={} empty={} too_short={} failed={} max_queue_depth={} processing_ms={} confidence_average={:.2} confidence_count={}",
            stats.queued,
            stats.completed,
            stats.emitted,
            stats.empty,
            stats.too_short,
            stats.failed,
            stats.max_queue_depth,
            stats.total_processing_duration.as_millis(),
            confidence_average,
            stats.confidence_count
        );
        if stats.failed > 0 {
            warn!(
                "transcription worker final failures={} queued={} completed={}",
                stats.failed, stats.queued, stats.completed
            );
        }
    } else if stats.failed > 0 {
        warn!(
            "transcription worker summary queued={} completed={} emitted={} empty={} too_short={} failed={} max_queue_depth={} processing_ms={} confidence_average={:.2} confidence_count={}",
            stats.queued,
            stats.completed,
            stats.emitted,
            stats.empty,
            stats.too_short,
            stats.failed,
            stats.max_queue_depth,
            stats.total_processing_duration.as_millis(),
            confidence_average,
            stats.confidence_count
        );
    } else {
        debug!(
            "transcription worker summary queued={} completed={} emitted={} empty={} too_short={} failed={} max_queue_depth={} processing_ms={} confidence_average={:.2} confidence_count={}",
            stats.queued,
            stats.completed,
            stats.emitted,
            stats.empty,
            stats.too_short,
            stats.failed,
            stats.max_queue_depth,
            stats.total_processing_duration.as_millis(),
            confidence_average,
            stats.confidence_count
        );
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranscriptUpdate {
    pub text: String,
    pub timestamp: String, // Wall-clock time for reference (e.g., "14:30:05")
    pub source: String,
    pub sequence_id: u64,
    pub chunk_start_time: f64, // Legacy field, kept for compatibility
    pub is_partial: bool,
    pub confidence: f32,
    // NEW: Recording-relative timestamps for playback sync
    pub audio_start_time: f64, // Seconds from recording start (e.g., 125.3)
    pub audio_end_time: f64,   // Seconds from recording start (e.g., 128.6)
    pub duration: f64,          // Segment duration in seconds (e.g., 3.3)
}

// NOTE: get_transcript_history and get_recording_meeting_name functions
// have been moved to recording_commands.rs where they have access to RECORDING_MANAGER

/// Optimized parallel transcription task ensuring ZERO chunk loss
pub fn start_transcription_task<R: Runtime>(
    app: AppHandle<R>,
    transcription_receiver: tokio::sync::mpsc::UnboundedReceiver<AudioChunk>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("Starting transcription worker task");

        let transcription_engine = match super::engine::get_or_init_transcription_engine(&app).await {
            Ok(engine) => engine,
            Err(e) => {
                error!("Failed to initialize transcription engine");
                let _ = app.emit("transcription-error", serde_json::json!({
                    "error": e,
                    "userMessage": "Recording failed: Unable to initialize speech recognition. Please check your model settings.",
                    "actionable": true
                }));
                return;
            }
        };

        const NUM_WORKERS: usize = 1;
        let (work_sender, work_receiver) = tokio::sync::mpsc::unbounded_channel::<AudioChunk>();
        let work_receiver = Arc::new(tokio::sync::Mutex::new(work_receiver));
        let chunks_queued = Arc::new(AtomicU64::new(0));
        let chunks_completed = Arc::new(AtomicU64::new(0));
        let mut worker_handles = Vec::new();

        for worker_id in 0..NUM_WORKERS {
            let engine_clone = match &transcription_engine {
                TranscriptionEngine::Whisper(e) => TranscriptionEngine::Whisper(e.clone()),
                TranscriptionEngine::Parakeet(e) => TranscriptionEngine::Parakeet(e.clone()),
                TranscriptionEngine::Provider(p) => TranscriptionEngine::Provider(p.clone()),
            };
            let app_clone = app.clone();
            let work_receiver_clone = work_receiver.clone();
            let chunks_completed_clone = chunks_completed.clone();
            let chunks_queued_clone = chunks_queued.clone();

            worker_handles.push(tokio::spawn(async move {
                let mut stats = WorkerLogStats::default();
                let mut last_summary = Instant::now();

                while let Some(chunk) = {
                    let mut receiver = work_receiver_clone.lock().await;
                    receiver.recv().await
                } {
                    stats.queued += 1;
                    let queued = chunks_queued_clone.load(Ordering::SeqCst);
                    let completed_before = chunks_completed_clone.load(Ordering::SeqCst);
                    stats.max_queue_depth = stats
                        .max_queue_depth
                        .max(queued.saturating_sub(completed_before));

                    let chunk_timestamp = chunk.timestamp;
                    let chunk_duration = chunk.data.len() as f64 / chunk.sample_rate as f64;
                    let processing_started = Instant::now();

                    if !engine_clone.is_model_loaded().await {
                        stats.failed += 1;
                    } else {
                        match transcribe_chunk_with_provider(&engine_clone, chunk, &app_clone).await {
                            Ok((transcript, confidence_opt, is_partial)) => {
                                if let Some(confidence) = confidence_opt {
                                    stats.confidence_total += confidence as f64;
                                    stats.confidence_count += 1;
                                }

                                if should_emit_transcript(&transcript) {
                                    if SPEECH_DETECTED_EMITTED
                                        .compare_exchange(
                                            false,
                                            true,
                                            Ordering::SeqCst,
                                            Ordering::SeqCst,
                                        )
                                        .is_ok()
                                        && app_clone
                                            .emit(
                                                "speech-detected",
                                                serde_json::json!({
                                                    "message": "Speech activity detected"
                                                }),
                                            )
                                            .is_err()
                                    {
                                        stats.failed += 1;
                                    }

                                    let sequence_id =
                                        SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst);
                                    let update = TranscriptUpdate {
                                        text: transcript,
                                        timestamp: format_current_timestamp(),
                                        source: "Audio".to_string(),
                                        sequence_id,
                                        chunk_start_time: chunk_timestamp,
                                        is_partial,
                                        confidence: confidence_opt.unwrap_or(0.85),
                                        audio_start_time: chunk_timestamp,
                                        audio_end_time: chunk_timestamp + chunk_duration,
                                        duration: chunk_duration,
                                    };

                                    if app_clone.emit("transcript-update", &update).is_ok() {
                                        stats.emitted += 1;
                                        #[cfg(debug_assertions)]
                                        if should_log_transcript_snippet(stats.emitted) {
                                            debug!(
                                                "accepted transcript result #{}: {}",
                                                stats.emitted,
                                                crate::utils::log_snippet(&update.text, 200)
                                            );
                                        }
                                    } else {
                                        stats.failed += 1;
                                    }
                                } else {
                                    stats.empty += 1;
                                }
                            }
                            Err(TranscriptionError::AudioTooShort { .. }) => {
                                stats.too_short += 1;
                            }
                            Err(TranscriptionError::ModelNotLoaded) => {
                                stats.failed += 1;
                            }
                            Err(error) => {
                                stats.failed += 1;
                                let _ = app_clone.emit("transcription-warning", error.to_string());
                            }
                        }
                    }

                    stats.total_processing_duration += processing_started.elapsed();
                    stats.completed += 1;
                    let completed = chunks_completed_clone.fetch_add(1, Ordering::SeqCst) + 1;
                    let progress_percentage = if queued > 0 {
                        (completed as f64 / queued as f64 * 100.0) as u32
                    } else {
                        100
                    };
                    let _ = app_clone.emit("transcription-progress", serde_json::json!({
                        "worker_id": worker_id,
                        "chunks_completed": completed,
                        "chunks_queued": queued,
                        "progress_percentage": progress_percentage,
                        "message": format!("Worker {} processing... ({}/{})", worker_id, completed, queued)
                    }));

                    if last_summary.elapsed() >= Duration::from_secs(60) {
                        emit_worker_log_snapshot(stats.snapshot_and_reset(), false);
                        last_summary = Instant::now();
                    }
                }

                emit_worker_log_snapshot(stats.snapshot_and_reset(), true);
            }));
        }

        let mut receiver = transcription_receiver;
        while let Some(chunk) = receiver.recv().await {
            chunks_queued.fetch_add(1, Ordering::SeqCst);
            if work_sender.send(chunk).is_err() {
                error!("Failed to send transcription chunk to worker");
                break;
            }
        }
        drop(work_sender);

        let total_chunks_queued = chunks_queued.load(Ordering::SeqCst);
        let _ = app.emit("transcription-queue-complete", serde_json::json!({
            "total_chunks": total_chunks_queued,
            "message": format!("{} chunks queued for processing - waiting for completion", total_chunks_queued)
        }));

        for handle in worker_handles {
            if handle.await.is_err() {
                error!("Transcription worker panicked");
            }
        }

        const MAX_VERIFICATION_ATTEMPTS: u32 = 10;
        for _ in 0..MAX_VERIFICATION_ATTEMPTS {
            if chunks_queued.load(Ordering::SeqCst) == chunks_completed.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let final_queued = chunks_queued.load(Ordering::SeqCst);
        let final_completed = chunks_completed.load(Ordering::SeqCst);
        if final_queued != final_completed {
            error!(
                "Transcription chunk loss detected queued={} completed={}",
                final_queued, final_completed
            );
            let _ = app.emit(
                "transcript-chunk-loss-detected",
                serde_json::json!({
                    "chunks_queued": final_queued,
                    "chunks_completed": final_completed,
                    "chunks_lost": final_queued - final_completed,
                    "message": "Some transcript chunks may have been lost during shutdown"
                }),
            );
        }

        info!(
            "Transcription worker task completed queued={} completed={}",
            final_queued, final_completed
        );
    })
}

/// Transcribe audio chunk using the appropriate provider (Whisper, Parakeet, or trait-based)
/// Returns: (text, confidence Option, is_partial)
async fn transcribe_chunk_with_provider<R: Runtime>(
    engine: &TranscriptionEngine,
    chunk: AudioChunk,
    app: &AppHandle<R>,
) -> std::result::Result<(String, Option<f32>, bool), TranscriptionError> {
    let speech_samples = if chunk.sample_rate != 16000 {
        crate::audio::audio_processing::resample_audio(&chunk.data, chunk.sample_rate, 16000)
    } else {
        chunk.data
    };

    if speech_samples.is_empty() {
        return Err(TranscriptionError::AudioTooShort {
            samples: 0,
            minimum: 1600,
        });
    }

    match engine {
        TranscriptionEngine::Whisper(whisper_engine) => {
            let language = crate::get_language_preference_internal();
            match whisper_engine
                .transcribe_audio_with_confidence(speech_samples, language)
                .await
            {
                Ok((text, confidence, is_partial)) => {
                    Ok((text.trim().to_string(), Some(confidence), is_partial))
                }
                Err(error) => {
                    let transcription_error = TranscriptionError::EngineFailed(error.to_string());
                    let _ = app.emit(
                        "transcription-error",
                        &serde_json::json!({
                            "error": transcription_error.to_string(),
                            "userMessage": format!("Transcription failed: {}", transcription_error),
                            "actionable": false
                        }),
                    );
                    Err(transcription_error)
                }
            }
        }
        TranscriptionEngine::Parakeet(parakeet_engine) => {
            match parakeet_engine.transcribe_audio(speech_samples).await {
                Ok(text) => Ok((text.trim().to_string(), None, false)),
                Err(error) => {
                    let transcription_error = TranscriptionError::EngineFailed(error.to_string());
                    let _ = app.emit(
                        "transcription-error",
                        &serde_json::json!({
                            "error": transcription_error.to_string(),
                            "userMessage": format!("Transcription failed: {}", transcription_error),
                            "actionable": false
                        }),
                    );
                    Err(transcription_error)
                }
            }
        }
        TranscriptionEngine::Provider(provider) => {
            let language = crate::get_language_preference_internal();
            match provider.transcribe(speech_samples, language).await {
                Ok(result) => Ok((
                    result.text.trim().to_string(),
                    result.confidence,
                    result.is_partial,
                )),
                Err(error) => {
                    let _ = app.emit(
                        "transcription-error",
                        &serde_json::json!({
                            "error": error.to_string(),
                            "userMessage": format!("Transcription failed: {}", error),
                            "actionable": false
                        }),
                    );
                    Err(error)
                }
            }
        }
    }
}

/// Format current timestamp (wall-clock time)
fn format_current_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();

    let hours = (now.as_secs() / 3600) % 24;
    let minutes = (now.as_secs() / 60) % 60;
    let seconds = now.as_secs() % 60;

    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

/// Format recording-relative time as [MM:SS]
#[allow(dead_code)]
fn format_recording_time(seconds: f64) -> String {
    let total_seconds = seconds.floor() as u64;
    let minutes = total_seconds / 60;
    let secs = total_seconds % 60;

    format!("[{:02}:{:02}]", minutes, secs)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn keeps_short_acknowledgements() {
            assert!(should_emit_transcript("Yes"));
            assert!(should_emit_transcript("ok"));
        }

        #[test]
        fn drops_empty_and_whitespace_only() {
            assert!(!should_emit_transcript(""));
            assert!(!should_emit_transcript("   "));
        }

        #[test]
        fn logging_worker_sampling_and_diagnostics_are_bounded() {
            for accepted_count in [1, 2, 3, 4, 5, 25, 50] {
                assert!(should_log_transcript_snippet(accepted_count));
            }
            for accepted_count in [6, 24, 26, 49] {
                assert!(!should_log_transcript_snippet(accepted_count));
            }

            let mut stats = WorkerLogStats::default();
            for _ in 0..3 {
                stats.failed += 1;
            }
            let snapshot = stats.snapshot_and_reset();
            assert_eq!(snapshot.failed, 3);
            assert_eq!(stats.failed, 0);
        }
    }
