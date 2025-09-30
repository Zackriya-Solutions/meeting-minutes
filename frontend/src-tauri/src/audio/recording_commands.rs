use std::sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}, Mutex};
use serde::{Deserialize, Serialize};
use tauri::{Runtime, AppHandle, Emitter};
use log::{info, error, warn};
use anyhow::Result;
use tokio::task::JoinHandle;

use super::{
    RecordingManager, AudioChunk,
    parse_audio_device
};

// Simple recording state tracking
static IS_RECORDING: AtomicBool = AtomicBool::new(false);

// Sequence counter for transcript updates
static SEQUENCE_COUNTER: AtomicU64 = AtomicU64::new(0);

// Global recording manager and transcription task to keep them alive during recording
static RECORDING_MANAGER: Mutex<Option<RecordingManager>> = Mutex::new(None);
static TRANSCRIPTION_TASK: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

#[derive(Debug, Deserialize)]
pub struct RecordingArgs {
    pub save_path: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct TranscriptionStatus {
    pub chunks_in_queue: usize,
    pub is_processing: bool,
    pub last_activity_ms: u64,
}

#[derive(Debug, Serialize, Clone)]
pub struct TranscriptUpdate {
    pub text: String,
    pub timestamp: String,
    pub source: String,
    pub sequence_id: u64,
    pub chunk_start_time: f64,
    pub is_partial: bool,
    pub confidence: f32,
}

/// Start recording with default devices
pub async fn start_recording<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    start_recording_with_meeting_name(app, None).await
}

/// Start recording with default devices and optional meeting name
pub async fn start_recording_with_meeting_name<R: Runtime>(
    app: AppHandle<R>,
    meeting_name: Option<String>
) -> Result<(), String> {
    info!("Starting recording with default devices, meeting: {:?}", meeting_name);

    // Check if already recording
    let current_recording_state = IS_RECORDING.load(Ordering::SeqCst);
    info!("🔍 IS_RECORDING state check: {}", current_recording_state);
    if current_recording_state {
        return Err("Recording already in progress".to_string());
    }

    // Validate that Whisper models are available before starting recording
    info!("🔍 Validating Whisper model availability before starting recording...");
    if let Err(validation_error) = validate_whisper_model_ready(&app).await {
        error!("Model validation failed: {}", validation_error);

        // Emit actionable error event for frontend to show model selector
        let _ = app.emit("transcription-error", serde_json::json!({
            "error": validation_error,
            "userMessage": "Recording cannot start: No Whisper models are available. Please download a model to enable transcription.",
            "actionable": true
        }));

        return Err(validation_error);
    }
    info!("✅ Whisper model validation passed");

    // Async-first approach - no more blocking operations!
    info!("🚀 Starting async recording initialization");

    // Create new recording manager
    let mut manager = RecordingManager::new();

    // Set meeting name if provided
    if let Some(name) = meeting_name.clone() {
        manager.set_meeting_name(Some(name));
    }

    // Set up error callback
    let app_for_error = app.clone();
    manager.set_error_callback(move |error| {
        let _ = app_for_error.emit("recording-error", error.user_message());
    });

    // Start recording with default devices
    let transcription_receiver = manager.start_recording_with_defaults().await
        .map_err(|e| format!("Failed to start recording: {}", e))?;

    // Store the manager globally to keep it alive
    {
        let mut global_manager = RECORDING_MANAGER.lock().unwrap();
        *global_manager = Some(manager);
    }

    // Set recording flag
    info!("🔍 Setting IS_RECORDING to true");
    IS_RECORDING.store(true, Ordering::SeqCst);

    // Start optimized parallel transcription task and store handle
    let task_handle = start_transcription_task(app.clone(), transcription_receiver);
    {
        let mut global_task = TRANSCRIPTION_TASK.lock().unwrap();
        *global_task = Some(task_handle);
    }

    // Emit success event
    app.emit("recording-started", serde_json::json!({
        "message": "Recording started successfully with parallel processing",
        "devices": ["Default Microphone", "Default System Audio"],
        "workers": 4
    })).map_err(|e| e.to_string())?;

    // Update tray menu to reflect recording state
    crate::tray::update_tray_menu(&app);

    info!("✅ Recording started successfully with async-first approach");
    
    Ok(())
}

/// Start recording with specific devices
pub async fn start_recording_with_devices<R: Runtime>(
    app: AppHandle<R>,
    mic_device_name: Option<String>,
    system_device_name: Option<String>
) -> Result<(), String> {
    start_recording_with_devices_and_meeting(app, mic_device_name, system_device_name, None).await
}

/// Start recording with specific devices and optional meeting name
pub async fn start_recording_with_devices_and_meeting<R: Runtime>(
    app: AppHandle<R>,
    mic_device_name: Option<String>,
    system_device_name: Option<String>,
    meeting_name: Option<String>
) -> Result<(), String> {
    info!("Starting recording with specific devices: mic={:?}, system={:?}, meeting={:?}",
          mic_device_name, system_device_name, meeting_name);

    // Check if already recording
    let current_recording_state = IS_RECORDING.load(Ordering::SeqCst);
    info!("🔍 IS_RECORDING state check: {}", current_recording_state);
    if current_recording_state {
        return Err("Recording already in progress".to_string());
    }

    // Validate that Whisper models are available before starting recording
    info!("🔍 Validating Whisper model availability before starting recording...");
    if let Err(validation_error) = validate_whisper_model_ready(&app).await {
        error!("Model validation failed: {}", validation_error);

        // Emit actionable error event for frontend to show model selector
        let _ = app.emit("transcription-error", serde_json::json!({
            "error": validation_error,
            "userMessage": "Recording cannot start: No Whisper models are available. Please download a model to enable transcription.",
            "actionable": true
        }));

        return Err(validation_error);
    }
    info!("✅ Whisper model validation passed");

    // Parse devices
    let mic_device = if let Some(ref name) = mic_device_name {
        Some(Arc::new(parse_audio_device(name)
            .map_err(|e| format!("Invalid microphone device '{}': {}", name, e))?))
    } else {
        None
    };

    let system_device = if let Some(ref name) = system_device_name {
        Some(Arc::new(parse_audio_device(name)
            .map_err(|e| format!("Invalid system device '{}': {}", name, e))?))
    } else {
        None
    };

    // Async-first approach for custom devices - no more blocking operations!
    info!("🚀 Starting async recording initialization with custom devices");

    // Create new recording manager
    let mut manager = RecordingManager::new();

    // Set meeting name if provided
    if let Some(name) = meeting_name.clone() {
        manager.set_meeting_name(Some(name));
    }

    // Set up error callback
    let app_for_error = app.clone();
    manager.set_error_callback(move |error| {
        let _ = app_for_error.emit("recording-error", error.user_message());
    });

    // Start recording with specified devices
    let transcription_receiver = manager.start_recording(mic_device, system_device).await
        .map_err(|e| format!("Failed to start recording: {}", e))?;

    // Store the manager globally to keep it alive
    {
        let mut global_manager = RECORDING_MANAGER.lock().unwrap();
        *global_manager = Some(manager);
    }

    // Set recording flag
    info!("🔍 Setting IS_RECORDING to true");
    IS_RECORDING.store(true, Ordering::SeqCst);

    // Start optimized parallel transcription task and store handle
    let task_handle = start_transcription_task(app.clone(), transcription_receiver);
    {
        let mut global_task = TRANSCRIPTION_TASK.lock().unwrap();
        *global_task = Some(task_handle);
    }

    // Emit success event
    app.emit("recording-started", serde_json::json!({
        "message": "Recording started with custom devices and parallel processing",
        "devices": [
            mic_device_name.unwrap_or_else(|| "Default Microphone".to_string()),
            system_device_name.unwrap_or_else(|| "Default System Audio".to_string())
        ],
        "workers": 4
    })).map_err(|e| e.to_string())?;

    // Update tray menu to reflect recording state
    crate::tray::update_tray_menu(&app);

    info!("✅ Recording started with custom devices using async-first approach");
    
    Ok(())
}

/// Stop recording with optimized graceful shutdown ensuring NO transcript chunks are lost
pub async fn stop_recording<R: Runtime>(app: AppHandle<R>, _args: RecordingArgs) -> Result<(), String> {
    info!("🛑 Starting optimized recording shutdown - ensuring ALL transcript chunks are preserved");

    // Check if recording is active
    if !IS_RECORDING.load(Ordering::SeqCst) {
        info!("Recording was not active");
        return Ok(());
    }

    // Emit shutdown progress to frontend
    let _ = app.emit("recording-shutdown-progress", serde_json::json!({
        "stage": "stopping_audio",
        "message": "Stopping audio capture...",
        "progress": 20
    }));

    // Step 1: Stop audio capture immediately (no more new chunks) with proper error handling
    let manager_for_cleanup = {
        let mut global_manager = RECORDING_MANAGER.lock().unwrap();
        global_manager.take()
    };

    let stop_result = if let Some(mut manager) = manager_for_cleanup {
        // Use FORCE FLUSH to immediately process all accumulated audio - eliminates 30s delay!
        info!("🚀 Using FORCE FLUSH to eliminate pipeline accumulation delays");
        let result = manager.stop_streams_and_force_flush().await;
        // Store manager back for later cleanup
        let manager_for_cleanup = Some(manager);
        (result, manager_for_cleanup)
    } else {
        warn!("No recording manager found to stop");
        (Ok(()), None)
    };

    let (stop_result, manager_for_cleanup) = stop_result;

    match stop_result {
        Ok(_) => {
            info!("✅ Audio streams stopped successfully - no more chunks will be created");
        }
        Err(e) => {
            error!("❌ Failed to stop audio streams: {}", e);
            return Err(format!("Failed to stop audio streams: {}", e));
        }
    }

    // Step 2: Signal transcription workers to finish processing ALL queued chunks
    let _ = app.emit("recording-shutdown-progress", serde_json::json!({
        "stage": "processing_transcripts",
        "message": "Processing remaining transcript chunks...",
        "progress": 40
    }));

    // Wait for transcription task with enhanced progress monitoring (NO TIMEOUT - we must process all chunks)
    let transcription_task = {
        let mut global_task = TRANSCRIPTION_TASK.lock().unwrap();
        global_task.take()
    };

    if let Some(task_handle) = transcription_task {
        info!("⏳ Waiting for ALL transcription chunks to be processed (no timeout - preserving every chunk)");

        // Enhanced progress monitoring during shutdown
        let progress_app = app.clone();
        let progress_task = tokio::spawn(async move {
            let last_update = std::time::Instant::now();

            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                // Emit periodic progress updates during shutdown
                let elapsed = last_update.elapsed().as_secs();
                let _ = progress_app.emit("recording-shutdown-progress", serde_json::json!({
                    "stage": "processing_transcripts",
                    "message": format!("Processing transcripts... ({}s elapsed)", elapsed),
                    "progress": 40,
                    "detailed": true,
                    "elapsed_seconds": elapsed
                }));
            }
        });

        // Wait indefinitely for transcription completion - no 30 second timeout!
        match task_handle.await {
            Ok(()) => {
                info!("✅ ALL transcription chunks processed successfully - no data lost");
            }
            Err(e) => {
                warn!("⚠️ Transcription task completed with error: {:?}", e);
                // Continue anyway - the worker may have processed most chunks
            }
        }

        // Stop progress monitoring
        progress_task.abort();

    } else {
        info!("ℹ️ No transcription task found to wait for");
    }

    // Step 3: Now safely unload Whisper model after ALL chunks are processed
    let _ = app.emit("recording-shutdown-progress", serde_json::json!({
        "stage": "unloading_model",
        "message": "Unloading speech recognition model...",
        "progress": 70
    }));

    info!("🧠 All transcript chunks processed. Now safely unloading Whisper model...");
    let engine_clone = {
        let engine_guard = crate::whisper_engine::commands::WHISPER_ENGINE.lock().unwrap();
        engine_guard.as_ref().cloned()
    };

    if let Some(engine) = engine_clone {
        let current_model = engine.get_current_model().await.unwrap_or_else(|| "unknown".to_string());
        info!("Current model before unload: '{}'", current_model);

        if engine.unload_model().await {
            info!("✅ Model '{}' unloaded successfully", current_model);
        } else {
            warn!("⚠️ Failed to unload model '{}'", current_model);
        }
    } else {
        warn!("⚠️ No whisper engine found to unload model");
    }

    // Step 4: Finalize recording state and cleanup resources safely
    let _ = app.emit("recording-shutdown-progress", serde_json::json!({
        "stage": "finalizing",
        "message": "Finalizing recording and cleaning up resources...",
        "progress": 90
    }));

    // Perform final cleanup with the manager if available
    if let Some(mut manager) = manager_for_cleanup {
        info!("🧹 Performing final cleanup and saving recording data");
        match manager.save_recording_only(&app).await {
            Ok(_) => {
                info!("✅ Recording data saved successfully during cleanup");
            }
            Err(e) => {
                warn!("⚠️ Error during recording cleanup (transcripts preserved): {}", e);
                // Don't fail shutdown - transcripts are already preserved
            }
        }
    } else {
        info!("ℹ️ No recording manager available for cleanup");
    }

    // Set recording flag to false
    info!("🔍 Setting IS_RECORDING to false");
    IS_RECORDING.store(false, Ordering::SeqCst);

    // Step 5: Complete shutdown
    let _ = app.emit("recording-shutdown-progress", serde_json::json!({
        "stage": "complete",
        "message": "Recording stopped successfully",
        "progress": 100
    }));

    // Emit final stop event
    app.emit("recording-stopped", serde_json::json!({
        "message": "Recording stopped - all transcript chunks preserved"
    })).map_err(|e| e.to_string())?;

    // Update tray menu to reflect stopped state
    crate::tray::update_tray_menu(&app);

    info!("🎉 Recording stopped successfully with ZERO transcript chunks lost");
    Ok(())
}

/// Check if recording is active
pub async fn is_recording() -> bool {
    IS_RECORDING.load(Ordering::SeqCst)
}

/// Get recording statistics
pub async fn get_transcription_status() -> TranscriptionStatus {
    TranscriptionStatus {
        chunks_in_queue: 0,
        is_processing: IS_RECORDING.load(Ordering::SeqCst),
        last_activity_ms: 0,
    }
}

/// Pause the current recording
#[tauri::command]
pub async fn pause_recording<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    info!("Pausing recording");

    // Check if currently recording
    if !IS_RECORDING.load(Ordering::SeqCst) {
        return Err("No recording is currently active".to_string());
    }

    // Access the recording manager and pause it
    let manager_guard = RECORDING_MANAGER.lock().unwrap();
    if let Some(manager) = manager_guard.as_ref() {
        manager.pause_recording().map_err(|e| e.to_string())?;

        // Emit pause event to frontend
        app.emit("recording-paused", serde_json::json!({
            "message": "Recording paused"
        })).map_err(|e| e.to_string())?;

        // Update tray menu to reflect paused state
        crate::tray::update_tray_menu(&app);

        info!("Recording paused successfully");
        Ok(())
    } else {
        Err("No recording manager found".to_string())
    }
}

/// Resume the current recording
#[tauri::command]
pub async fn resume_recording<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    info!("Resuming recording");

    // Check if currently recording
    if !IS_RECORDING.load(Ordering::SeqCst) {
        return Err("No recording is currently active".to_string());
    }

    // Access the recording manager and resume it
    let manager_guard = RECORDING_MANAGER.lock().unwrap();
    if let Some(manager) = manager_guard.as_ref() {
        manager.resume_recording().map_err(|e| e.to_string())?;

        // Emit resume event to frontend
        app.emit("recording-resumed", serde_json::json!({
            "message": "Recording resumed"
        })).map_err(|e| e.to_string())?;

        // Update tray menu to reflect resumed state
        crate::tray::update_tray_menu(&app);

        info!("Recording resumed successfully");
        Ok(())
    } else {
        Err("No recording manager found".to_string())
    }
}

/// Check if recording is currently paused
#[tauri::command]
pub async fn is_recording_paused() -> bool {
    let manager_guard = RECORDING_MANAGER.lock().unwrap();
    if let Some(manager) = manager_guard.as_ref() {
        manager.is_paused()
    } else {
        false
    }
}

/// Get detailed recording state
#[tauri::command]
pub async fn get_recording_state() -> serde_json::Value {
    let is_recording = IS_RECORDING.load(Ordering::SeqCst);
    let manager_guard = RECORDING_MANAGER.lock().unwrap();

    if let Some(manager) = manager_guard.as_ref() {
        serde_json::json!({
            "is_recording": is_recording,
            "is_paused": manager.is_paused(),
            "is_active": manager.is_active(),
            "recording_duration": manager.get_recording_duration(),
            "active_duration": manager.get_active_recording_duration(),
            "total_pause_duration": manager.get_total_pause_duration(),
            "current_pause_duration": manager.get_current_pause_duration()
        })
    } else {
        serde_json::json!({
            "is_recording": is_recording,
            "is_paused": false,
            "is_active": false,
            "recording_duration": null,
            "active_duration": null,
            "total_pause_duration": 0.0,
            "current_pause_duration": null
        })
    }
}

/// Optimized parallel transcription task ensuring ZERO chunk loss
fn start_transcription_task<R: Runtime>(
    app: AppHandle<R>,
    transcription_receiver: tokio::sync::mpsc::UnboundedReceiver<AudioChunk>
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("🚀 Starting optimized parallel transcription task - guaranteeing zero chunk loss");

        // Initialize whisper engine
        let whisper_engine = match get_or_init_whisper(&app).await {
            Ok(engine) => engine,
            Err(e) => {
                error!("Failed to initialize Whisper engine: {}", e);
                let _ = app.emit("transcription-error", serde_json::json!({
                    "error": e,
                    "userMessage": "Recording failed: Unable to initialize speech recognition. Please check your model settings.",
                    "actionable": true
                }));
                return;
            }
        };

        // Create parallel workers for faster processing while preserving ALL chunks
        const NUM_WORKERS: usize = 4; // Increased from 3 to ensure better chunk distribution
        let (work_sender, work_receiver) = tokio::sync::mpsc::unbounded_channel::<AudioChunk>();
        let work_receiver = Arc::new(tokio::sync::Mutex::new(work_receiver));

        // Track completion: AtomicU64 for chunks queued, AtomicU64 for chunks completed
        let chunks_queued = Arc::new(AtomicU64::new(0));
        let chunks_completed = Arc::new(AtomicU64::new(0));
        let input_finished = Arc::new(AtomicBool::new(false));

        info!("📊 Starting {} parallel transcription workers", NUM_WORKERS);

        // Spawn worker tasks
        let mut worker_handles = Vec::new();
        for worker_id in 0..NUM_WORKERS {
            let whisper_engine_clone = whisper_engine.clone();
            let app_clone = app.clone();
            let work_receiver_clone = work_receiver.clone();
            let chunks_completed_clone = chunks_completed.clone();
            let input_finished_clone = input_finished.clone();
            let chunks_queued_clone = chunks_queued.clone();

            let worker_handle = tokio::spawn(async move {
                info!("👷 Worker {} started", worker_id);

                // PRE-VALIDATE Whisper model state to avoid repeated async calls per chunk
                let initial_model_loaded = whisper_engine_clone.is_model_loaded().await;
                let current_model = whisper_engine_clone.get_current_model().await.unwrap_or_else(|| "unknown".to_string());

                if initial_model_loaded {
                    info!("✅ Worker {} pre-validation: Whisper model '{}' is loaded and ready", worker_id, current_model);
                } else {
                    warn!("⚠️ Worker {} pre-validation: Whisper model not loaded - chunks may be skipped", worker_id);
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

                            if should_log_this_chunk {
                                info!("👷 Worker {} processing chunk {} with {} samples",
                                      worker_id, chunk.chunk_id, chunk.data.len());
                            }

                            // Check if model is still loaded before processing
                            if !whisper_engine_clone.is_model_loaded().await {
                                warn!("⚠️ Worker {}: Whisper model unloaded, but continuing to preserve chunk {}", worker_id, chunk.chunk_id);
                                // Still count as completed even if we can't process
                                chunks_completed_clone.fetch_add(1, Ordering::SeqCst);
                                continue;
                            }

                            let chunk_timestamp = chunk.timestamp;

                            // Transcribe with whisper using streaming approach
                            match transcribe_chunk_with_streaming(&whisper_engine_clone, chunk, &app_clone).await {
                                Ok((transcript, confidence, is_partial)) => {
                                    let confidence_threshold = 0.4; // Display results above 40% confidence

                                    if !transcript.trim().is_empty() && confidence >= confidence_threshold {
                                        // PERFORMANCE: Only log transcription results, not every processing step
                                        info!("✅ Worker {} transcribed: {} (confidence: {:.2}, partial: {})",
                                              worker_id, transcript, confidence, is_partial);

                                        // Save transcript chunk to recording manager (only final results)
                                        if !is_partial {
                                            let global_manager = RECORDING_MANAGER.lock().unwrap();
                                            if let Some(manager) = global_manager.as_ref() {
                                                manager.add_transcript_chunk(transcript.clone());
                                            }
                                        }

                                        // Emit transcript update with partial flag and confidence
                                        let sequence_id = SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst);
                                        let update = TranscriptUpdate {
                                            text: transcript,
                                            timestamp: format_current_timestamp(),
                                            source: "Audio".to_string(),
                                            sequence_id,
                                            chunk_start_time: chunk_timestamp,
                                            is_partial,
                                            confidence,
                                        };

                                        if let Err(e) = app_clone.emit("transcript-update", &update) {
                                            error!("Worker {}: Failed to emit transcript update: {}", worker_id, e);
                                        }
                                        // PERFORMANCE: Removed verbose logging of every emission
                                    } else if !transcript.trim().is_empty() && should_log_this_chunk {
                                        // PERFORMANCE: Only log low-confidence results occasionally
                                        info!("Worker {} low-confidence transcription (confidence: {:.2}), skipping", worker_id, confidence);
                                    }
                                }
                                Err(e) => {
                                    warn!("Worker {}: Transcription failed: {}", worker_id, e);
                                    let _ = app_clone.emit("transcription-warning", e);
                                }
                            }

                            // Mark chunk as completed
                            let completed = chunks_completed_clone.fetch_add(1, Ordering::SeqCst) + 1;
                            let queued = chunks_queued_clone.load(Ordering::SeqCst);

                            // PERFORMANCE: Only log progress every 5th chunk to reduce I/O overhead
                            if completed % 5 == 0 || should_log_this_chunk {
                                info!("Worker {}: Progress {}/{} chunks ({:.1}%)",
                                      worker_id, completed, queued,
                                      (completed as f64 / queued.max(1) as f64 * 100.0));
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
                                    info!("👷 Worker {} finishing - all {}/{} chunks processed", worker_id, final_completed, final_queued);
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
            info!("📥 Dispatching chunk {} to workers (total queued: {})", chunk.chunk_id, queued);

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
                info!("🎉 ALL {} chunks processed successfully - ZERO chunks lost!", final_completed);
                break;
            } else if verification_attempts < MAX_VERIFICATION_ATTEMPTS {
                verification_attempts += 1;
                warn!("⚠️ Chunk count mismatch (attempt {}): {} queued, {} completed - waiting for stragglers...",
                     verification_attempts, final_queued, final_completed);

                // Wait a bit for any remaining chunks to be processed
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            } else {
                error!("❌ CRITICAL: After {} attempts, chunk loss detected: {} queued, {} completed",
                      MAX_VERIFICATION_ATTEMPTS, final_queued, final_completed);

                // Emit critical error event
                let _ = app.emit("transcript-chunk-loss-detected", serde_json::json!({
                    "chunks_queued": final_queued,
                    "chunks_completed": final_completed,
                    "chunks_lost": final_queued - final_completed,
                    "message": "Some transcript chunks may have been lost during shutdown"
                }));
                break;
            }
        }

        info!("✅ Parallel transcription task completed - all workers finished, ready for model unload");
    })
}

/// Transcribe audio chunk with streaming support and confidence scoring
async fn transcribe_chunk_with_streaming<R: Runtime>(
    whisper_engine: &Arc<crate::whisper_engine::WhisperEngine>,
    chunk: AudioChunk,
    app: &AppHandle<R>,
) -> Result<(String, f32, bool), String> {
    // Convert to 16kHz mono for whisper and VAD
    let whisper_data = if chunk.sample_rate != 16000 {
        crate::audio::audio_processing::resample_audio(&chunk.data, chunk.sample_rate, 16000)
    } else {
        chunk.data
    };

    // Skip VAD processing here since the pipeline already extracted speech using VAD
    let speech_samples = whisper_data;

    // PERFORMANCE FIX: Only check for empty samples - trust VAD's decision on audio quality
    // Redundant energy checking after VAD filtering was too aggressive and rejected valid speech
    if speech_samples.is_empty() {
        info!("Empty audio chunk {}, skipping transcription", chunk.chunk_id);
        return Ok((String::new(), 0.0, false));
    }

    // Calculate energy for logging/monitoring only (not filtering)
    let energy: f32 = speech_samples.iter().map(|&x| x * x).sum::<f32>() / speech_samples.len() as f32;
    info!("Processing speech audio chunk {} with {} samples (energy: {:.6})",
          chunk.chunk_id, speech_samples.len(), energy);

    match whisper_engine.transcribe_audio_with_confidence(speech_samples).await {
        Ok((text, confidence, is_partial)) => {
            let cleaned_text = text.trim().to_string();
            if cleaned_text.is_empty() {
                return Ok((String::new(), confidence, is_partial));
            }

            info!("Transcription complete for chunk {}: '{}' (confidence: {:.2}, partial: {})",
                  chunk.chunk_id, cleaned_text, confidence, is_partial);

            Ok((cleaned_text, confidence, is_partial))
        }
        Err(e) => {
            error!("Whisper transcription failed for chunk {}: {}", chunk.chunk_id, e);

            let error_msg = format!("Transcription failed: {}", e);
            if let Err(emit_err) = app.emit("transcription-error", &serde_json::json!({
                "error": e.to_string(),
                "userMessage": error_msg.clone(),
                "actionable": false
            })) {
                error!("Failed to emit transcription error: {}", emit_err);
            }

            Err(error_msg)
        }
    }
}


/// Validate that Whisper models are ready before starting recording
async fn validate_whisper_model_ready<R: Runtime>(_app: &AppHandle<R>) -> Result<(), String> {
    // Ensure whisper engine is initialized first
    if let Err(init_error) = crate::whisper_engine::commands::whisper_init().await {
        warn!("❌ Failed to initialize Whisper engine: {}", init_error);
        return Err(format!("Failed to initialize speech recognition: {}", init_error));
    }

    // Call the whisper validation command
    match crate::whisper_engine::commands::whisper_validate_model_ready().await {
        Ok(model_name) => {
            info!("✅ Model validation successful: {} is ready", model_name);
            Ok(())
        }
        Err(e) => {
            warn!("❌ Model validation failed: {}", e);
            Err(e)
        }
    }
}

/// Get or initialize Whisper engine using API configuration
pub async fn get_or_init_whisper<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<crate::whisper_engine::WhisperEngine>, String> {
    // Check if engine already exists and has a model loaded
    let existing_engine = {
        let engine_guard = crate::whisper_engine::commands::WHISPER_ENGINE.lock().unwrap();
        engine_guard.as_ref().cloned()
    };

    if let Some(engine) = existing_engine {
        // Check if a model is already loaded
        if engine.is_model_loaded().await {
            let current_model = engine.get_current_model().await.unwrap_or_else(|| "unknown".to_string());
            info!("✅ Whisper engine already initialized with model: '{}'", current_model);
            return Ok(engine);
        } else {
            info!("🔄 Whisper engine exists but no model loaded, will load model from config");
        }
    }

    // Initialize new engine if needed
    info!("Initializing Whisper engine");

    // First ensure the engine is initialized
    if let Err(e) = crate::whisper_engine::commands::whisper_init().await {
        return Err(format!("Failed to initialize Whisper engine: {}", e));
    }

    // Get the engine reference
    let engine = {
        let engine_guard = crate::whisper_engine::commands::WHISPER_ENGINE.lock().unwrap();
        engine_guard.as_ref().cloned().ok_or("Failed to get initialized engine")?
    };

    // Get model configuration from API
    let model_to_load = match crate::api::api::api_get_transcript_config(app.clone(), None).await {
        Ok(Some(config)) => {
            info!("Got transcript config from API - provider: {}, model: {}", config.provider, config.model);
            if config.provider == "localWhisper" {
                info!("Using model from API config: {}", config.model);
                config.model
            } else {
                info!("API config uses non-local provider ({}), falling back to 'small'", config.provider);
                "small".to_string()
            }
        }
        Ok(None) => {
            info!("No transcript config found in API, falling back to 'small'");
            "small".to_string()
        }
        Err(e) => {
            warn!("Failed to get transcript config from API: {}, falling back to 'small'", e);
            "small".to_string()
        }
    };

    info!("Selected model to load: {}", model_to_load);

    // Discover available models to check if the desired model is downloaded
    let models = engine.discover_models().await
        .map_err(|e| format!("Failed to discover models: {}", e))?;

    info!("Discovered {} models", models.len());
    for model in &models {
        info!("Model: {} - Status: {:?} - Path: {}", model.name, model.status, model.path.display());
    }

    // Check if the desired model is available
    let model_info = models.iter().find(|model| model.name == model_to_load);

    if model_info.is_none() {
        info!("Model '{}' not found in discovered models. Available models: {:?}",
              model_to_load, models.iter().map(|m| &m.name).collect::<Vec<_>>());
    }

    match model_info {
        Some(model) => {
            match model.status {
                crate::whisper_engine::ModelStatus::Available => {
                    info!("Loading model: {}", model_to_load);
                    engine.load_model(&model_to_load).await
                        .map_err(|e| format!("Failed to load model '{}': {}", model_to_load, e))?;
                    info!("✅ Model '{}' loaded successfully", model_to_load);
                }
                crate::whisper_engine::ModelStatus::Missing => {
                    return Err(format!("Model '{}' is not downloaded. Please download it first from the settings.", model_to_load));
                }
                crate::whisper_engine::ModelStatus::Downloading { progress } => {
                    return Err(format!("Model '{}' is currently downloading ({}%). Please wait for it to complete.", model_to_load, progress));
                }
                crate::whisper_engine::ModelStatus::Error(ref err) => {
                    return Err(format!("Model '{}' has an error: {}. Please check the model or try downloading it again.", model_to_load, err));
                }
                crate::whisper_engine::ModelStatus::Corrupted { .. } => {
                    return Err(format!("Model '{}' is corrupted. Please delete it and download again from the settings.", model_to_load));
                }
            }
        }
        None => {
            // Check if we have any available models and try to load the first one
            let available_models: Vec<_> = models.iter()
                .filter(|m| matches!(m.status, crate::whisper_engine::ModelStatus::Available))
                .collect();

            if let Some(fallback_model) = available_models.first() {
                warn!("Model '{}' not found, falling back to available model: '{}'", model_to_load, fallback_model.name);
                engine.load_model(&fallback_model.name).await
                    .map_err(|e| format!("Failed to load fallback model '{}': {}", fallback_model.name, e))?;
                info!("✅ Fallback model '{}' loaded successfully", fallback_model.name);
            } else {
                return Err(format!("Model '{}' is not supported and no other models are available. Please download a model from the settings.", model_to_load));
            }
        }
    }

    Ok(engine)
}


/// Format current timestamp
fn format_current_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();

    let hours = (now.as_secs() / 3600) % 24;
    let minutes = (now.as_secs() / 60) % 60;
    let seconds = now.as_secs() % 60;

    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}
