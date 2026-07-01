// audio/transcription/worker.rs
//
// Parallel transcription worker pool and chunk processing logic.

use super::engine::TranscriptionEngine;
use super::provider::TranscriptionError;
use crate::audio::AudioChunk;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Runtime};

// Sequence counter for transcript updates
static SEQUENCE_COUNTER: AtomicU64 = AtomicU64::new(0);

// Speech detection flag - reset per recording session
static SPEECH_DETECTED_EMITTED: AtomicBool = AtomicBool::new(false);

/// Reset the speech detected flag for a new recording session
pub fn reset_speech_detected_flag() {
    SPEECH_DETECTED_EMITTED.store(false, Ordering::SeqCst);
    info!("\u{1f50d} SPEECH_DETECTED_EMITTED reset to: {}", SPEECH_DETECTED_EMITTED.load(Ordering::SeqCst));
}

// Transcription pause flag - flipped mid-recording by the UI to suspend
// chunk processing without ending the recording. Like SPEECH_DETECTED_EMITTED,
// it is a session-scoped lifecycle signal: reset on every new recording start.
static TRANSCRIPTION_PAUSED: AtomicBool = AtomicBool::new(false);

// Buffered chunks during pause - transcribe when resumed or at recording end
static PAUSED_CHUNKS: Mutex<Vec<AudioChunk>> = Mutex::new(Vec::new());

/// Set the transcription-paused flag. When true, the worker buffers incoming
/// chunks instead of transcribing them. When set to false, buffered chunks
/// are transcribed in order.
pub fn set_transcription_paused(paused: bool) {
    TRANSCRIPTION_PAUSED.store(paused, Ordering::SeqCst);
    info!("\u{23f8} TRANSCRIPTION_PAUSED set to: {}", paused);
    
    // If unpausing, process buffered chunks in background
    if !paused {
        // Buffered chunks will be picked up by the worker loop on next iteration
    }
}

/// Read the current transcription-paused flag. Used by the UI to sync state
/// on mount so the button label reflects the worker's actual mode.
pub fn is_transcription_paused() -> bool {
    TRANSCRIPTION_PAUSED.load(Ordering::SeqCst)
}

/// Reset the transcription-paused flag and clear buffered chunks for a new recording session.
pub fn reset_transcription_paused_flag() {
    TRANSCRIPTION_PAUSED.store(false, Ordering::SeqCst);
    if let Ok(mut chunks) = PAUSED_CHUNKS.lock() {
        chunks.clear();
    }
    info!("\u{23f8} TRANSCRIPTION_PAUSED reset to: {}", TRANSCRIPTION_PAUSED.load(Ordering::SeqCst));
}

/// Drain and return all buffered paused chunks for processing
pub fn drain_paused_chunks() -> Vec<AudioChunk> {
    PAUSED_CHUNKS.lock().map(|mut c| std::mem::take(&mut *c)).unwrap_or_default()
}

/// Drain any chunks buffered during a transcription pause and transcribe them
/// using the currently-active engine. Called from `stop_recording` so that audio
/// captured while transcription was paused is transcribed before the model is
/// unloaded and the worker task is torn down.
///
/// Returns the number of chunks actually transcribed (0 means nothing was
/// buffered or the provider is unavailable).
///
/// The dispatch is inlined (rather than re-using `transcribe_chunk_with_provider`)
/// because that helper is intentionally module-private — wiring it through here
/// would expand the API surface for a single caller. The Match arms below mirror
/// it 1:1.
pub async fn drain_and_transcribe_paused_chunks<R: Runtime>(
    app: &AppHandle<R>,
) -> usize {
    let buffered = drain_paused_chunks();
    if buffered.is_empty() {
        return 0;
    }

    info!(
        "\u{23f8} drain_and_transcribe_paused_chunks: flushing {} buffered chunks at recording end",
        buffered.len()
    );

    // Acquire the live engine the same way the worker task did. This works even
    // after the worker has already exited because the engine is kept in a global
    // until step 3 of stop_recording unloads it.
    let engine = match super::engine::get_or_init_transcription_engine(app).await {
        Ok(e) => e,
        Err(e) => {
            error!(
                "drain_and_transcribe_paused_chunks: failed to init engine for post-recording drain: {}",
                e
            );
            return 0;
        }
    };

    if !engine.is_model_loaded().await {
        warn!(
            "drain_and_transcribe_paused_chunks: model already unloaded — {} buffered chunks will be lost",
            buffered.len()
        );
        return 0;
    }

    let mut transcribed = 0usize;
    for chunk in buffered {
        let chunk_id = chunk.chunk_id;
        let chunk_duration = chunk.data.len() as f64 / chunk.sample_rate as f64;
        // Mirror transcribe_chunk_with_provider's behaviour: skip empty audio,
        // dispatch on engine variant, emit transcript-update on success.
        let res: std::result::Result<(String, Option<f32>, bool), TranscriptionError> = (|| async {
            let speech_samples = if chunk.sample_rate != 16000 {
                crate::audio::audio_processing::resample_audio(&chunk.data, chunk.sample_rate, 16000)
            } else {
                chunk.data
            };
            if speech_samples.is_empty() {
                return Err(TranscriptionError::AudioTooShort { samples: 0, minimum: 1600 });
            }
            match &engine {
                TranscriptionEngine::Whisper(w) => {
                    let language = crate::get_language_preference_internal();
                    match w.transcribe_audio_with_confidence(speech_samples, language).await {
                        Ok((text, confidence, is_partial)) => {
                            Ok((text.trim().to_string(), Some(confidence), is_partial))
                        }
                        Err(e) => Err(TranscriptionError::EngineFailed(e.to_string())),
                    }
                }
                TranscriptionEngine::Parakeet(p) => match p.transcribe_audio(speech_samples).await {
                    Ok(text) => Ok((text.trim().to_string(), None, false)),
                    Err(e) => Err(TranscriptionError::EngineFailed(e.to_string())),
                },
                TranscriptionEngine::Provider(provider) => {
                    let language = crate::get_language_preference_internal();
                    match provider.transcribe(speech_samples, language).await {
                        Ok(result) => Ok((
                            result.text.trim().to_string(),
                            result.confidence,
                            result.is_partial,
                        )),
                        Err(e) => Err(e),
                    }
                }
            }
        })().await;

        match res {
            Ok((text, confidence_opt, is_partial)) => {
                let confidence_threshold = match &engine {
                    TranscriptionEngine::Whisper(_) | TranscriptionEngine::Provider(_) => 0.3,
                    TranscriptionEngine::Parakeet(_) => 0.0,
                };
                let meets_threshold = confidence_opt.map_or(true, |c| c >= confidence_threshold);
                if text.trim().is_empty() || !meets_threshold {
                    continue;
                }
                let sequence_id = SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst);
                let update = TranscriptUpdate {
                    text,
                    timestamp: format_current_timestamp(),
                    source: "Audio".to_string(),
                    sequence_id,
                    chunk_start_time: chunk.timestamp,
                    is_partial,
                    confidence: confidence_opt.unwrap_or(0.85),
                    audio_start_time: chunk.timestamp,
                    audio_end_time: chunk.timestamp + chunk_duration,
                    duration: chunk_duration,
                };
                if let Err(e) = app.emit("transcript-update", &update) {
                    error!(
                        "drain_and_transcribe_paused_chunks: failed to emit transcript-update for chunk {}: {}",
                        chunk_id, e
                    );
                } else {
                    transcribed += 1;
                    info!(
                        "\u{2705} drain_and_transcribe_paused_chunks: emitted buffered chunk {} ({} seq)",
                        chunk_id, sequence_id
                    );
                }
            }
            Err(e) => {
                warn!(
                    "drain_and_transcribe_paused_chunks: chunk {} transcription failed: {}",
                    chunk_id, e
                );
            }
        }
    }
    info!(
        "\u{1f389} drain_and_transcribe_paused_chunks: drained and processed {} buffered paused chunks",
        transcribed
    );
    transcribed
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
        info!("🚀 Starting optimized parallel transcription task - guaranteeing zero chunk loss");

        // Initialize transcription engine (Whisper or Parakeet based on config)
        let transcription_engine = match super::engine::get_or_init_transcription_engine(&app).await {
            Ok(engine) => engine,
            Err(e) => {
                error!("Failed to initialize transcription engine: {}", e);
                let _ = app.emit("transcription-error", serde_json::json!({
                    "error": e,
                    "userMessage": "Recording failed: Unable to initialize speech recognition. Please check your model settings.",
                    "actionable": true
                }));
                return;
            }
        };

        // Create parallel workers for faster processing while preserving ALL chunks
        const NUM_WORKERS: usize = 1; // Serial processing ensures transcripts emit in chronological order
        let (work_sender, work_receiver) = tokio::sync::mpsc::unbounded_channel::<AudioChunk>();
        let work_receiver = Arc::new(tokio::sync::Mutex::new(work_receiver));

        // Track completion: AtomicU64 for chunks queued, AtomicU64 for chunks completed
        let chunks_queued = Arc::new(AtomicU64::new(0));
        let chunks_completed = Arc::new(AtomicU64::new(0));
        let input_finished = Arc::new(AtomicBool::new(false));

        info!("📊 Starting {} transcription worker{} (serial mode for ordered emission)", NUM_WORKERS, if NUM_WORKERS == 1 { "" } else { "s" });

        // Spawn worker tasks
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
            let input_finished_clone = input_finished.clone();
            let chunks_queued_clone = chunks_queued.clone();

            let worker_handle = tokio::spawn(async move {
                info!("👷 Worker {} started", worker_id);

                // PRE-VALIDATE model state to avoid repeated async calls per chunk
                let initial_model_loaded = engine_clone.is_model_loaded().await;
                let current_model = engine_clone
                    .get_current_model()
                    .await
                    .unwrap_or_else(|| "unknown".to_string());

                let engine_name = engine_clone.provider_name();

                if initial_model_loaded {
                    info!(
                        "✅ Worker {} pre-validation: {} model '{}' is loaded and ready",
                        worker_id, engine_name, current_model
                    );
                } else {
                    warn!("⚠️ Worker {} pre-validation: {} model not loaded - chunks may be skipped", worker_id, engine_name);
                }

                loop {
                    // Try to get a chunk to process
                    let chunk = {
                        let mut receiver = work_receiver_clone.lock().await;
                        receiver.recv().await
                    };

                    match chunk {
                                            Some(chunk) => {
                                                // PERFORMANCE OPTIMIZATION: Reduce logging in hot path
                                                // Only log every 10th chunk per worker to reduce I/O overhead
                                                let should_log_this_chunk = chunk.chunk_id % 10 == 0;

                                                // In-session pause toggle: while TRANSCRIPTION_PAUSED is set
                                                // by the UI, the worker buffers incoming chunks instead of
                                                // transcribing them. Buffered chunks are processed when
                                                // transcription is resumed or at recording end.
                                                if TRANSCRIPTION_PAUSED.load(Ordering::SeqCst) {
                                                                                                    // Buffer the chunk for later transcription
                                                                                                    let chunk_id = chunk.chunk_id; // Log before move
                                                                                                    if let Ok(mut paused_chunks) = PAUSED_CHUNKS.lock() {
                                                                                                        paused_chunks.push(chunk);
                                                                                                    }
                                                                                                    if should_log_this_chunk {
                                                                                                        info!(
                                                                                                            "⏸ Worker {} buffering chunk {} while transcription is paused",
                                                                                                            worker_id, chunk_id
                                                                                                        );
                                                                                                    }
                                                                                                    // Don't increment completed counter - we'll process these later
                                                                                                    continue;
                                                                                                }

                                                // Check if there are buffered paused chunks to process first
                                                let paused_chunks = drain_paused_chunks();
                                                if !paused_chunks.is_empty() {
                                                    info!("▶ Worker {} processing {} buffered paused chunks", worker_id, paused_chunks.len());
                                                    for pchunk in paused_chunks {
                                                        // Process each buffered chunk
                                                        if !engine_clone.is_model_loaded().await {
                                                            warn!("⚠️ Worker {}: Model unloaded while processing buffered chunks", worker_id);
                                                            break;
                                                        }
                                                        let _ = transcribe_chunk_with_provider(&engine_clone, pchunk, &app_clone).await;
                                                    }
                                                }

                                                if should_log_this_chunk {
                                                    info!(
                                                        "👷 Worker {} processing chunk {} with {} samples",
                                                        worker_id,
                                                        chunk.chunk_id,
                                                        chunk.data.len()
                                                    );
                                                }

                            // Check if model is still loaded before processing
                            if !engine_clone.is_model_loaded().await {
                                warn!("⚠️ Worker {}: Model unloaded, but continuing to preserve chunk {}", worker_id, chunk.chunk_id);
                                // Still count as completed even if we can't process
                                chunks_completed_clone.fetch_add(1, Ordering::SeqCst);
                                continue;
                            }

                            let chunk_timestamp = chunk.timestamp;
                            let chunk_duration = chunk.data.len() as f64 / chunk.sample_rate as f64;

                            // Transcribe with provider-agnostic approach
                            match transcribe_chunk_with_provider(
                                &engine_clone,
                                chunk,
                                &app_clone,
                            )
                            .await
                            {
                                Ok((transcript, confidence_opt, is_partial)) => {
                                    // Provider-aware confidence threshold
                                    let confidence_threshold = match &engine_clone {
                                        TranscriptionEngine::Whisper(_) | TranscriptionEngine::Provider(_) => 0.3,
                                        TranscriptionEngine::Parakeet(_) => 0.0, // Parakeet has no confidence, accept all
                                    };

                                    let confidence_str = match confidence_opt {
                                        Some(c) => format!("{:.2}", c),
                                        None => "N/A".to_string(),
                                    };

                                    info!("🔍 Worker {} transcription result: text='{}', confidence={}, partial={}, threshold={:.2}",
                                          worker_id, transcript, confidence_str, is_partial, confidence_threshold);

                                    // Check confidence threshold (or accept if no confidence provided)
                                    let meets_threshold = confidence_opt.map_or(true, |c| c >= confidence_threshold);

                                    if !transcript.trim().is_empty() && meets_threshold {
                                        // PERFORMANCE: Only log transcription results, not every processing step
                                        info!("✅ Worker {} transcribed: {} (confidence: {}, partial: {})",
                                              worker_id, transcript, confidence_str, is_partial);

                                        // Emit speech-detected event for frontend UX (only on first detection per session)
                                        // This is lightweight and provides better user feedback
                                        let current_flag = SPEECH_DETECTED_EMITTED.load(Ordering::SeqCst);
                                        info!("🔍 Checking speech-detected flag: current={}, will_emit={}", current_flag, !current_flag);

                                        if !current_flag {
                                            SPEECH_DETECTED_EMITTED.store(true, Ordering::SeqCst);
                                            match app_clone.emit("speech-detected", serde_json::json!({
                                                "message": "Speech activity detected"
                                            })) {
                                                Ok(_) => info!("🎤 ✅ First speech detected - successfully emitted speech-detected event"),
                                                Err(e) => error!("🎤 ❌ Failed to emit speech-detected event: {}", e),
                                            }
                                        } else {
                                            info!("🔍 Speech already detected in this session, not re-emitting");
                                        }

                                        // Generate sequence ID and calculate timestamps FIRST
                                        let sequence_id = SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst);
                                        let audio_start_time = chunk_timestamp; // Already in seconds from recording start
                                        let audio_end_time = chunk_timestamp + chunk_duration;

                                        // Save structured transcript segment to recording manager (only final results)
                                        // Save ALL segments (partial and final) to ensure complete JSON
                                        // Create structured segment with full timestamp data
                                        // NOTE: This is now handled via the transcript-update event emission below
                                        // The recording_commands module listens to these events and saves them
                                        // This decouples the transcription worker from direct RECORDING_MANAGER access

                                        // Emit transcript update with NEW recording-relative timestamps

                                        let update = TranscriptUpdate {
                                            text: transcript,
                                            timestamp: format_current_timestamp(), // Wall-clock for reference
                                            source: "Audio".to_string(),
                                            sequence_id,
                                            chunk_start_time: chunk_timestamp, // Legacy compatibility
                                            is_partial,
                                            confidence: confidence_opt.unwrap_or(0.85), // Default for providers without confidence
                                            // NEW: Recording-relative timestamps for sync
                                            audio_start_time,
                                            audio_end_time,
                                            duration: chunk_duration,
                                        };

                                        if let Err(e) = app_clone.emit("transcript-update", &update)
                                        {
                                            error!(
                                                "Worker {}: Failed to emit transcript update: {}",
                                                worker_id, e
                                            );
                                        }
                                        // PERFORMANCE: Removed verbose logging of every emission
                                    } else if !transcript.trim().is_empty() && should_log_this_chunk
                                    {
                                        // PERFORMANCE: Only log low-confidence results occasionally
                                        if let Some(c) = confidence_opt {
                                            info!("Worker {} low-confidence transcription (confidence: {:.2}), skipping", worker_id, c);
                                        }
                                    }
                                }
                                Err(e) => {
                                    // Improved error handling with specific cases
                                    match e {
                                        TranscriptionError::AudioTooShort { .. } => {
                                            // Skip silently, this is expected for very short chunks
                                            info!("Worker {}: {}", worker_id, e);
                                            chunks_completed_clone.fetch_add(1, Ordering::SeqCst);
                                            continue;
                                        }
                                        TranscriptionError::ModelNotLoaded => {
                                            warn!("Worker {}: Model unloaded during transcription", worker_id);
                                            chunks_completed_clone.fetch_add(1, Ordering::SeqCst);
                                            continue;
                                        }
                                        _ => {
                                            warn!("Worker {}: Transcription failed: {}", worker_id, e);
                                            let _ = app_clone.emit("transcription-warning", e.to_string());
                                        }
                                    }
                                }
                            }

                            // Mark chunk as completed
                            let completed =
                                chunks_completed_clone.fetch_add(1, Ordering::SeqCst) + 1;
                            let queued = chunks_queued_clone.load(Ordering::SeqCst);

                            // PERFORMANCE: Only log progress every 5th chunk to reduce I/O overhead
                            if completed % 5 == 0 || should_log_this_chunk {
                                info!(
                                    "Worker {}: Progress {}/{} chunks ({:.1}%)",
                                    worker_id,
                                    completed,
                                    queued,
                                    (completed as f64 / queued.max(1) as f64 * 100.0)
                                );
                            }

                            // Emit progress event for frontend
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
                        }
                        None => {
                            // No more chunks available
                            if input_finished_clone.load(Ordering::SeqCst) {
                                // Double-check that all queued chunks are actually completed
                                let final_queued = chunks_queued_clone.load(Ordering::SeqCst);
                                let final_completed = chunks_completed_clone.load(Ordering::SeqCst);

                                if final_completed >= final_queued {
                                    info!(
                                        "👷 Worker {} finishing - all {}/{} chunks processed",
                                        worker_id, final_completed, final_queued
                                    );
                                    break;
                                } else {
                                    warn!("👷 Worker {} detected potential chunk loss: {}/{} completed, waiting...", worker_id, final_completed, final_queued);
                                    // AGGRESSIVE POLLING: Reduced from 50ms to 5ms for faster chunk detection during shutdown
                                    tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
                                }
                            } else {
                                // AGGRESSIVE POLLING: Reduced from 10ms to 1ms for faster response during shutdown
                                tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                            }
                        }
                    }
                }

                info!("👷 Worker {} completed", worker_id);
            });

            worker_handles.push(worker_handle);
        }

        // Main dispatcher: receive chunks and distribute to workers
        let mut receiver = transcription_receiver;
        while let Some(chunk) = receiver.recv().await {
            let queued = chunks_queued.fetch_add(1, Ordering::SeqCst) + 1;
            info!(
                "📥 Dispatching chunk {} to workers (total queued: {})",
                chunk.chunk_id, queued
            );

            if let Err(_) = work_sender.send(chunk) {
                error!("❌ Failed to send chunk to workers - this should not happen!");
                break;
            }
        }

        // Signal that input is finished
        input_finished.store(true, Ordering::SeqCst);
        drop(work_sender); // Close the channel to signal workers

        let total_chunks_queued = chunks_queued.load(Ordering::SeqCst);
        info!("📭 Input finished with {} total chunks queued. Waiting for all {} workers to complete...",
              total_chunks_queued, NUM_WORKERS);

        // Emit final chunk count to frontend
        let _ = app.emit("transcription-queue-complete", serde_json::json!({
            "total_chunks": total_chunks_queued,
            "message": format!("{} chunks queued for processing - waiting for completion", total_chunks_queued)
        }));

        // Wait for all workers to complete
        for (worker_id, handle) in worker_handles.into_iter().enumerate() {
            if let Err(e) = handle.await {
                error!("❌ Worker {} panicked: {:?}", worker_id, e);
            } else {
                info!("✅ Worker {} completed successfully", worker_id);
            }
        }

        // Final verification with retry logic to catch any stragglers
        let mut verification_attempts = 0;
        const MAX_VERIFICATION_ATTEMPTS: u32 = 10;

        loop {
            let final_queued = chunks_queued.load(Ordering::SeqCst);
            let final_completed = chunks_completed.load(Ordering::SeqCst);

            if final_queued == final_completed {
                info!(
                    "🎉 ALL {} chunks processed successfully - ZERO chunks lost!",
                    final_completed
                );
                break;
            } else if verification_attempts < MAX_VERIFICATION_ATTEMPTS {
                verification_attempts += 1;
                warn!("⚠️ Chunk count mismatch (attempt {}): {} queued, {} completed - waiting for stragglers...",
                     verification_attempts, final_queued, final_completed);

                // Wait a bit for any remaining chunks to be processed
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            } else {
                error!(
                    "❌ CRITICAL: After {} attempts, chunk loss detected: {} queued, {} completed",
                    MAX_VERIFICATION_ATTEMPTS, final_queued, final_completed
                );

                // Emit critical error event
                let _ = app.emit(
                    "transcript-chunk-loss-detected",
                    serde_json::json!({
                        "chunks_queued": final_queued,
                        "chunks_completed": final_completed,
                        "chunks_lost": final_queued - final_completed,
                        "message": "Some transcript chunks may have been lost during shutdown"
                    }),
                );
                break;
            }
        }

        info!("✅ Parallel transcription task completed - all workers finished, ready for model unload");
    })
}

/// Transcribe audio chunk using the appropriate provider (Whisper, Parakeet, or trait-based)
/// Returns: (text, confidence Option, is_partial)
async fn transcribe_chunk_with_provider<R: Runtime>(
    engine: &TranscriptionEngine,
    chunk: AudioChunk,
    app: &AppHandle<R>,
) -> std::result::Result<(String, Option<f32>, bool), TranscriptionError> {
    // Convert to 16kHz mono for transcription
    let transcription_data = if chunk.sample_rate != 16000 {
        crate::audio::audio_processing::resample_audio(&chunk.data, chunk.sample_rate, 16000)
    } else {
        chunk.data
    };

    // Skip VAD processing here since the pipeline already extracted speech using VAD
    let speech_samples = transcription_data;

    // Check for empty samples - improved error handling
    if speech_samples.is_empty() {
        warn!(
            "Audio chunk {} is empty, skipping transcription",
            chunk.chunk_id
        );
        return Err(TranscriptionError::AudioTooShort {
            samples: 0,
            minimum: 1600, // 100ms at 16kHz
        });
    }

    // Calculate energy for logging/monitoring only
    let energy: f32 =
        speech_samples.iter().map(|&x| x * x).sum::<f32>() / speech_samples.len() as f32;
    info!(
        "Processing speech audio chunk {} with {} samples (energy: {:.6})",
        chunk.chunk_id,
        speech_samples.len(),
        energy
    );

    // Transcribe using the appropriate engine (with improved error handling)
    match engine {
        TranscriptionEngine::Whisper(whisper_engine) => {
            // Get language preference from global state
            let language = crate::get_language_preference_internal();

            match whisper_engine
                .transcribe_audio_with_confidence(speech_samples, language)
                .await
            {
                Ok((text, confidence, is_partial)) => {
                    let cleaned_text = text.trim().to_string();
                    if cleaned_text.is_empty() {
                        return Ok((String::new(), Some(confidence), is_partial));
                    }

                    info!(
                        "Whisper transcription complete for chunk {}: '{}' (confidence: {:.2}, partial: {})",
                        chunk.chunk_id, cleaned_text, confidence, is_partial
                    );

                    Ok((cleaned_text, Some(confidence), is_partial))
                }
                Err(e) => {
                    error!(
                        "Whisper transcription failed for chunk {}: {}",
                        chunk.chunk_id, e
                    );

                    let transcription_error = TranscriptionError::EngineFailed(e.to_string());
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
                Ok(text) => {
                    let cleaned_text = text.trim().to_string();
                    if cleaned_text.is_empty() {
                        return Ok((String::new(), None, false));
                    }

                    info!(
                        "Parakeet transcription complete for chunk {}: '{}'",
                        chunk.chunk_id, cleaned_text
                    );

                    // Parakeet doesn't provide confidence or partial results
                    Ok((cleaned_text, None, false))
                }
                Err(e) => {
                    error!(
                        "Parakeet transcription failed for chunk {}: {}",
                        chunk.chunk_id, e
                    );

                    let transcription_error = TranscriptionError::EngineFailed(e.to_string());
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
            // NEW: Trait-based provider (clean, unified interface)
            let language = crate::get_language_preference_internal();

            match provider.transcribe(speech_samples, language).await {
                Ok(result) => {
                    let cleaned_text = result.text.trim().to_string();
                    if cleaned_text.is_empty() {
                        return Ok((String::new(), result.confidence, result.is_partial));
                    }

                    let confidence_str = match result.confidence {
                        Some(c) => format!("confidence: {:.2}", c),
                        None => "no confidence".to_string(),
                    };

                    info!(
                        "{} transcription complete for chunk {}: '{}' ({}, partial: {})",
                        provider.provider_name(),
                        chunk.chunk_id,
                        cleaned_text,
                        confidence_str,
                        result.is_partial
                    );

                    Ok((cleaned_text, result.confidence, result.is_partial))
                }
                Err(e) => {
                    error!(
                        "{} transcription failed for chunk {}: {}",
                        provider.provider_name(),
                        chunk.chunk_id,
                        e
                    );

                    let _ = app.emit(
                        "transcription-error",
                        &serde_json::json!({
                            "error": e.to_string(),
                            "userMessage": format!("Transcription failed: {}", e),
                            "actionable": false
                        }),
                    );

                    Err(e)
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
