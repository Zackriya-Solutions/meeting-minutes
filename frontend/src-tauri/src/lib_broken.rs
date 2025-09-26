use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use tauri_plugin_notification::NotificationExt;

// Declare modules
pub mod audio;
pub mod ollama;
pub mod analytics;
pub mod api;
pub mod utils;
pub mod console_utils;
pub mod tray;
pub mod whisper_engine;
pub mod openrouter;

// Import components
use audio::{
    list_audio_devices, parse_audio_device, encode_single_audio, AudioDevice, DeviceType,
    RecordingArgs, TranscriptionStatus, TranscriptUpdate
};
use analytics::{AnalyticsClient, AnalyticsConfig};
use tauri::{Runtime, AppHandle, Emitter};
use tauri_plugin_store::StoreExt;
use log::{info, error, debug, warn};
use whisper_engine::{WhisperEngine, ModelInfo, ModelStatus};

// Global state - minimal (moved to respective modules)
// WHISPER_ENGINE is now in whisper_engine::commands
// ANALYTICS_CLIENT is now in analytics::commands

// ===== TAURI COMMAND HANDLERS =====
// These are thin wrappers that delegate to the audio module

/// Start recording with default devices
#[tauri::command]
async fn start_recording<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    audio::recording_commands::start_recording(app).await
}

/// Start recording with specific devices
#[tauri::command]
async fn start_recording_with_devices<R: Runtime>(
    app: AppHandle<R>,
    mic_device_name: Option<String>,
    system_device_name: Option<String>
) -> Result<(), String> {
    audio::recording_commands::start_recording_with_devices(app, mic_device_name, system_device_name).await
}

/// Stop recording
#[tauri::command]
async fn stop_recording<R: Runtime>(app: AppHandle<R>, args: RecordingArgs) -> Result<(), String> {
    audio::recording_commands::stop_recording(app, args).await
}

/// Check if recording is active
#[tauri::command]
async fn is_recording() -> bool {
    audio::recording_commands::is_recording().await
}

/// Get recording statistics
#[tauri::command]
async fn get_transcription_status() -> TranscriptionStatus {
    audio::recording_commands::get_transcription_status().await
}

// ===== FILE OPERATIONS =====

#[tauri::command]
fn read_audio_file(file_path: String) -> Result<Vec<u8>, String> {
    match std::fs::read(&file_path) {
        Ok(data) => Ok(data),
        Err(e) => Err(format!("Failed to read audio file: {}", e))
    }
}

#[tauri::command]
async fn save_transcript(file_path: String, content: String) -> Result<(), String> {
    info!("Saving transcript to: {}", file_path);
    tokio::fs::write(&file_path, content).await
        .map_err(|e| format!("Failed to save transcript: {}", e))
}

#[tauri::command]
async fn get_audio_devices() -> Result<Vec<AudioDevice>, String> {
    list_audio_devices().await.map_err(|e| format!("Failed to list audio devices: {}", e))
}

// ===== WHISPER ENGINE COMMANDS =====

// Whisper commands are now in whisper_engine::commands module

// ===== ANALYTICS COMMANDS =====
// Analytics commands are now in analytics::commands module

// ===== MAIN APPLICATION SETUP =====

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .setup(|app| {
            info!("Simplified audio app initialized");

            // Initialize system tray
            if let Err(e) = tray::create_tray(app.handle()) {
                error!("Failed to create system tray: {}", e);
            }

            // Trigger microphone permission request on startup
            if let Err(e) = audio::core::trigger_audio_permission() {
                error!("Failed to trigger audio permission: {}", e);
            }

            // Initialize whisper engine in background
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = whisper_engine::commands::whisper_init().await {
                    error!("Failed to initialize Whisper engine on startup: {}", e);
                    let _ = app_handle.emit("startup-error", format!("Whisper initialization failed: {}", e));
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Core recording commands
            start_recording,
            start_recording_with_devices,
            stop_recording,
            is_recording,
            get_transcription_status,

            // Audio device management
            get_audio_devices,

            // File operations
            read_audio_file,
            save_transcript,

            // Whisper engine commands
            whisper_engine::commands::whisper_init,
            whisper_engine::commands::whisper_get_available_models,
            whisper_engine::commands::whisper_load_model,
            whisper_engine::commands::whisper_get_current_model,
            whisper_engine::commands::whisper_is_model_loaded,
            whisper_engine::commands::whisper_transcribe_audio,
            whisper_engine::commands::whisper_get_models_directory,
            whisper_engine::commands::whisper_download_model,
            whisper_engine::commands::whisper_cancel_download,

            // Analytics commands
            analytics::commands::init_analytics,
            analytics::commands::disable_analytics,
            analytics::commands::track_event,
            analytics::commands::identify_user,
            analytics::commands::track_meeting_started,
            analytics::commands::track_recording_started,
            analytics::commands::track_recording_stopped,
            analytics::commands::track_meeting_deleted,
            analytics::commands::track_search_performed,
            analytics::commands::track_settings_changed,
            analytics::commands::track_feature_used,
            analytics::commands::is_analytics_enabled,
            analytics::commands::start_analytics_session,
            analytics::commands::end_analytics_session,
            analytics::commands::track_daily_active_user,
            analytics::commands::track_user_first_launch,
            analytics::commands::is_analytics_session_active,
            analytics::commands::track_summary_generation_started,
            analytics::commands::track_summary_generation_completed,
            analytics::commands::track_summary_regenerated,
            analytics::commands::track_model_changed,
            analytics::commands::track_custom_prompt_used,

            // API commands
            api::api_get_meetings,
            api::api_search_transcripts,
            api::api_get_profile,
            api::api_save_profile,
            api::api_update_profile,
            api::api_get_model_config,
            api::api_save_model_config,
            api::api_get_api_key,
            api::api_get_transcript_config,
            api::api_save_transcript_config,
            api::api_get_transcript_api_key,
            api::api_delete_meeting,
            api::api_get_meeting,
            api::api_save_meeting_title,
            api::api_save_meeting_summary,
            api::api_get_summary,
            api::api_save_transcript,
            api::api_process_transcript,
            api::test_backend_connection,
            api::debug_backend_connection,
            api::open_external_url,

            // Ollama commands
            ollama::get_ollama_models,

            // OpenRouter commands
            openrouter::commands::get_openrouter_models,

            // Console utils commands
            console_utils::commands::show_console,
            console_utils::commands::hide_console,
            console_utils::commands::toggle_console,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}