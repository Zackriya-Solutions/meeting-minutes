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
    if IS_RECORDING.load(Ordering::SeqCst) {
        return Err("Recording already in progress".to_string());
    }

    // Spawn the recording task in a separate thread to avoid Send issues
    let app_clone = app.clone();
    let meeting_name_clone = meeting_name.clone();
    tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async {
            // Create new recording manager
            let mut manager = RecordingManager::new();

            // Set meeting name if provided
            if let Some(name) = meeting_name_clone {
                manager.set_meeting_name(Some(name));
            }

            // Set up error callback
            let app_for_error = app_clone.clone();
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
            IS_RECORDING.store(true, Ordering::SeqCst);

            // Start transcription task and store handle
            let task_handle = start_transcription_task(app_clone.clone(), transcription_receiver);
            {
                let mut global_task = TRANSCRIPTION_TASK.lock().unwrap();
                *global_task = Some(task_handle);
            }

            // Emit success event
            app_clone.emit("recording-started", serde_json::json!({
                "message": "Recording started successfully",
                "devices": ["Default Microphone", "Default System Audio"]
            })).map_err(|e| e.to_string())?;

            info!("Recording started successfully");
            Ok::<(), String>(())
        })
    }).await.map_err(|e| format!("Task join error: {}", e))??;
    
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
    if IS_RECORDING.load(Ordering::SeqCst) {
        return Err("Recording already in progress".to_string());
    }

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

    // Spawn the recording task in a separate thread to avoid Send issues
    let app_clone = app.clone();
    let meeting_name_clone = meeting_name.clone();
    tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async {
            // Create new recording manager
            let mut manager = RecordingManager::new();

            // Set meeting name if provided
            if let Some(name) = meeting_name_clone {
                manager.set_meeting_name(Some(name));
            }

            // Set up error callback
            let app_for_error = app_clone.clone();
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
            IS_RECORDING.store(true, Ordering::SeqCst);

            // Start transcription task and store handle
            let task_handle = start_transcription_task(app_clone.clone(), transcription_receiver);
            {
                let mut global_task = TRANSCRIPTION_TASK.lock().unwrap();
                *global_task = Some(task_handle);
            }

            // Emit success event
            app_clone.emit("recording-started", serde_json::json!({
                "message": "Recording started with custom devices",
                "devices": [
                    mic_device_name.unwrap_or_else(|| "Default Microphone".to_string()),
                    system_device_name.unwrap_or_else(|| "Default System Audio".to_string())
                ]
            })).map_err(|e| e.to_string())?;

            info!("Recording started with custom devices");
            Ok::<(), String>(())
        })
    }).await.map_err(|e| format!("Task join error: {}", e))??;
    
    Ok(())
}

/// Stop recording
pub async fn stop_recording<R: Runtime>(app: AppHandle<R>, _args: RecordingArgs) -> Result<(), String> {
    info!("Stopping recording");

    // Check if recording is active
    if !IS_RECORDING.load(Ordering::SeqCst) {
        info!("Recording was not active");
        return Ok(());
    }

    // Stop the recording manager in a blocking task to avoid Send issues
    let app_clone = app.clone();
    let stop_result = tokio::task::spawn_blocking(move || {
        let mut global_manager = RECORDING_MANAGER.lock().unwrap();
        if let Some(mut manager) = global_manager.take() {
            // Use a blocking async runtime for the stop operation
            tokio::runtime::Handle::current().block_on(manager.stop_recording(&app_clone))
        } else {
            warn!("No recording manager found to stop");
            Ok(())
        }
    }).await.map_err(|e| format!("Task join error: {}", e))?;

    // Wait for transcription task to complete all pending chunks
    let transcription_task = {
        let mut global_task = TRANSCRIPTION_TASK.lock().unwrap();
        global_task.take()
    };

    if let Some(task_handle) = transcription_task {
        info!("Waiting for transcription task to complete all pending chunks...");
        match tokio::time::timeout(
            std::time::Duration::from_secs(30), // 30 second timeout
            task_handle
        ).await {
            Ok(Ok(())) => {
                info!("Transcription task completed successfully");
            }
            Ok(Err(e)) => {
                warn!("Transcription task completed with error: {:?}", e);
            }
            Err(_) => {
                warn!("Transcription task timed out after 30 seconds");
            }
        }
    } else {
        info!("No transcription task found to wait for");
    }

    match stop_result {
        Ok(_) => {
            info!("Recording manager stopped successfully");
        }
        Err(e) => {
            error!("Failed to stop recording: {}", e);
            return Err(format!("Failed to stop recording: {}", e));
        }
    }

    // Unload the whisper model to free memory (like old implementation)
    info!("Unloading whisper model to free memory...");
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

    // Set recording flag to false
    IS_RECORDING.store(false, Ordering::SeqCst);

    // Emit stop event
    app.emit("recording-stopped", serde_json::json!({
        "message": "Recording stopped"
    })).map_err(|e| e.to_string())?;

    info!("Recording stopped successfully");
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

/// Transcription task with simplified error handling
fn start_transcription_task<R: Runtime>(
    app: AppHandle<R>,
    mut transcription_receiver: tokio::sync::mpsc::UnboundedReceiver<AudioChunk>
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("Starting simplified transcription task");

        // Initialize whisper engine
        let whisper_engine = match get_or_init_whisper(&app).await {
            Ok(engine) => engine,
            Err(e) => {
                error!("Failed to initialize Whisper engine: {}", e);
                // Emit error to frontend with user-friendly message
                let _ = app.emit("transcription-error", serde_json::json!({
                    "error": e,
                    "userMessage": "Recording failed: Unable to initialize speech recognition. Please check your model settings.",
                    "actionable": true
                }));
                return;
            }
        };

        // Process transcription chunks
        while let Some(chunk) = transcription_receiver.recv().await {
            info!("Processing transcription chunk {} with {} samples",
                  chunk.chunk_id, chunk.data.len());

            // Store timestamp before moving chunk
            let chunk_timestamp = chunk.timestamp;

            // Transcribe with whisper
            match transcribe_chunk(&whisper_engine, chunk, &app).await {
                Ok(transcript) => {
                    if !transcript.trim().is_empty() {
                        info!("Transcription result: {}", transcript);

                        // Save transcript chunk to recording manager
                        {
                            let global_manager = RECORDING_MANAGER.lock().unwrap();
                            if let Some(manager) = global_manager.as_ref() {
                                manager.add_transcript_chunk(transcript.clone());
                            }
                        }

                        // Emit transcript update
                        let sequence_id = SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst);
                        let update = TranscriptUpdate {
                            text: transcript,
                            timestamp: format_current_timestamp(),
                            source: "Audio".to_string(),
                            sequence_id,
                            chunk_start_time: chunk_timestamp,
                            is_partial: false,
                        };

                        if let Err(e) = app.emit("transcript-update", &update) {
                            error!("Failed to emit transcript update: {}", e);
                        } else {
                            info!("Successfully emitted transcript-update with sequence_id: {}", sequence_id);
                        }
                    }
                }
                Err(e) => {
                    warn!("Transcription failed: {}", e);
                    // Emit error but continue processing
                    let _ = app.emit("transcription-warning", e);
                }
            }
        }

        info!("✅ Transcription task completed - all pending chunks processed");
    })
}

/// Transcribe a single audio chunk with VAD filtering and error handling
async fn transcribe_chunk<R: Runtime>(
    whisper_engine: &Arc<crate::whisper_engine::WhisperEngine>,
    chunk: AudioChunk,
    _app: &AppHandle<R>,
) -> Result<String, String> {
    // Convert to 16kHz mono for whisper and VAD
    let whisper_data = if chunk.sample_rate != 16000 {
        crate::audio::audio_processing::resample_audio(&chunk.data, chunk.sample_rate, 16000)
    } else {
        chunk.data
    };

    // Skip VAD processing here since the pipeline already extracted speech using VAD
    // The incoming chunk.data already contains VAD-processed speech segments
    let speech_samples = whisper_data;

    // Basic energy check to avoid transcribing very quiet audio
    if speech_samples.is_empty() {
        info!("Empty audio chunk {}, skipping transcription", chunk.chunk_id);
        return Ok(String::new());
    }

    // Check energy level of the audio
    let energy: f32 = speech_samples.iter().map(|&x| x * x).sum::<f32>() / speech_samples.len() as f32;
    if energy < 0.00001 { // Very low energy threshold
        info!("Very low energy audio in chunk {} (energy: {:.6}), skipping transcription", chunk.chunk_id, energy);
        return Ok(String::new());
    }

    info!("Processing speech audio chunk {} with {} samples (energy: {:.6})",
          chunk.chunk_id, speech_samples.len(), energy);

    // Check if we have enough speech content for transcription
    // Whisper needs at least 1 second of audio for reliable transcription
    let speech_duration_ms = (speech_samples.len() as f64 / 16000.0) * 1000.0;

    // Use old implementation's approach: pad short chunks instead of rejecting them
    let final_data = if speech_samples.len() < 16000 { // Less than 1 second of actual speech
        info!("Speech chunk {} too short ({:.1}ms), padding to 1 second", 
              chunk.chunk_id, speech_duration_ms);
        
        // Pad with silence to reach minimum 1 second (like old implementation)
        let mut padded_chunk = speech_samples;
        padded_chunk.resize(16000, 0.0); // Pad with silence
        padded_chunk
    } else {
        info!("Speech chunk {} has {} samples ({:.1}ms) - sufficient for whisper", 
              chunk.chunk_id, speech_samples.len(), speech_duration_ms);
        speech_samples
    };

    // Get current model name for logging
    let current_model = whisper_engine.get_current_model().await.unwrap_or_else(|| "unknown".to_string());
    info!("🎯 Transcribing chunk {} using model: '{}'", chunk.chunk_id, current_model);

    // Transcribe with timeout and error handling
    match tokio::time::timeout(
        std::time::Duration::from_secs(30), // 30 second timeout
        whisper_engine.transcribe_audio(final_data)
    ).await {
        Ok(result) => {
            match result {
                Ok(transcript) => {
                    info!("✅ Transcription successful for chunk {} using model '{}': '{}'", 
                          chunk.chunk_id, current_model, transcript.trim());
                    Ok(transcript)
                }
                Err(e) => {
                    error!("❌ Transcription failed for chunk {} using model '{}': {}", 
                           chunk.chunk_id, current_model, e);
                    Err(format!("Whisper transcription failed: {}", e))
                }
            }
        }
        Err(_) => {
            let error_msg = "Transcription timeout - chunk took too long to process";
            warn!("{}", error_msg);
            Err(error_msg.to_string())
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
