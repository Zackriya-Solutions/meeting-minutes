use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use serde::{Deserialize, Serialize};
use tauri::{Runtime, AppHandle, Emitter};
use log::{info, error, debug, warn};
use anyhow::Result;

use super::{
    RecordingManager, AudioChunk, AudioError,
    default_input_device, default_output_device,
    parse_audio_device, AudioDevice, DeviceType
};

// Simple recording state tracking
static IS_RECORDING: AtomicBool = AtomicBool::new(false);

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
}

/// Start recording with default devices
pub async fn start_recording<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    info!("Starting recording with default devices");

    // Check if already recording
    if IS_RECORDING.load(Ordering::SeqCst) {
        return Err("Recording already in progress".to_string());
    }

    // Spawn the recording task in a separate thread to avoid Send issues
    let app_clone = app.clone();
    tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async {
            // Create new recording manager
            let mut manager = RecordingManager::new();
            
            // Set up error callback
            let app_for_error = app_clone.clone();
            manager.set_error_callback(move |error| {
                let _ = app_for_error.emit("recording-error", error.user_message());
            });

            // Start recording with default devices
            let transcription_receiver = manager.start_recording_with_defaults().await
                .map_err(|e| format!("Failed to start recording: {}", e))?;

            // Set recording flag
            IS_RECORDING.store(true, Ordering::SeqCst);

            // Start transcription task
            start_transcription_task(app_clone.clone(), transcription_receiver);

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
    info!("Starting recording with specific devices: mic={:?}, system={:?}", 
          mic_device_name, system_device_name);

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
    tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async {
            // Create new recording manager
            let mut manager = RecordingManager::new();
            
            // Set up error callback
            let app_for_error = app_clone.clone();
            manager.set_error_callback(move |error| {
                let _ = app_for_error.emit("recording-error", error.user_message());
            });

            // Start recording with specified devices
            let transcription_receiver = manager.start_recording(mic_device, system_device).await
                .map_err(|e| format!("Failed to start recording: {}", e))?;

            // Set recording flag
            IS_RECORDING.store(true, Ordering::SeqCst);

            // Start transcription task
            start_transcription_task(app_clone.clone(), transcription_receiver);

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
async fn start_transcription_task<R: Runtime>(
    app: AppHandle<R>, 
    mut transcription_receiver: tokio::sync::mpsc::UnboundedReceiver<AudioChunk>
) {
    tokio::spawn(async move {
        info!("Starting simplified transcription task");

        // Initialize whisper engine
        let whisper_engine = match get_or_init_whisper().await {
            Ok(engine) => engine,
            Err(e) => {
                error!("Failed to initialize Whisper engine: {}", e);
                // Emit error to frontend
                let _ = app.emit("transcription-error", e);
                return;
            }
        };

        // Process transcription chunks
        while let Some(chunk) = transcription_receiver.recv().await {
            info!("Processing transcription chunk {} with {} samples",
                  chunk.chunk_id, chunk.data.len());

            // Transcribe with whisper
            match transcribe_chunk(&whisper_engine, chunk, &app).await {
                Ok(transcript) => {
                    if !transcript.trim().is_empty() {
                        info!("Transcription result: {}", transcript);

                        // Emit transcript update
                        let update = TranscriptUpdate {
                            text: transcript,
                            timestamp: format_current_timestamp(),
                            source: "Audio".to_string(),
                        };

                        if let Err(e) = app.emit("transcript-update", update) {
                            error!("Failed to emit transcript update: {}", e);
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

        info!("Transcription task ended");
    });
}

/// Transcribe a single audio chunk with error handling
async fn transcribe_chunk<R: Runtime>(
    whisper_engine: &Arc<crate::whisper_engine::WhisperEngine>,
    chunk: AudioChunk,
    _app: &AppHandle<R>,
) -> Result<String, String> {
    // Convert to 16kHz mono for whisper
    let whisper_data = if chunk.sample_rate != 16000 {
        crate::audio::audio_processing::resample_audio(&chunk.data, chunk.sample_rate, 16000)
    } else {
        chunk.data
    };

    // Ensure minimum length (1 second = 16000 samples at 16kHz)
    let final_data = if whisper_data.len() < 16000 {
        let mut padded = whisper_data;
        padded.resize(16000, 0.0);
        padded
    } else {
        whisper_data
    };

    // Transcribe with timeout and error handling
    match tokio::time::timeout(
        std::time::Duration::from_secs(30), // 30 second timeout
        whisper_engine.transcribe_audio(final_data)
    ).await {
        Ok(result) => result.map_err(|e| format!("Whisper transcription failed: {}", e)),
        Err(_) => {
            let error_msg = "Transcription timeout - chunk took too long to process";
            warn!("{}", error_msg);
            Err(error_msg.to_string())
        }
    }
}

/// Get or initialize Whisper engine
pub async fn get_or_init_whisper() -> Result<Arc<crate::whisper_engine::WhisperEngine>, String> {
    // Check if engine already exists
    {
        let engine_guard = crate::whisper_engine::commands::WHISPER_ENGINE.lock().unwrap();
        if let Some(engine) = engine_guard.as_ref() {
            return Ok(engine.clone());
        }
    }

    // Initialize new engine
    info!("Initializing Whisper engine");
    let new_engine = crate::whisper_engine::WhisperEngine::new()
        .map_err(|e| format!("Failed to create Whisper engine: {}", e))?;

    // Load default model
    let engine = new_engine;
    engine.load_model("large-v3").await
        .map_err(|e| format!("Failed to load default model: {}", e))?;

    let engine_arc = Arc::new(engine);
    
    // Store the engine
    {
        let mut engine_guard = crate::whisper_engine::commands::WHISPER_ENGINE.lock().unwrap();
        *engine_guard = Some(engine_arc.clone());
    }
    
    Ok(engine_arc)
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
