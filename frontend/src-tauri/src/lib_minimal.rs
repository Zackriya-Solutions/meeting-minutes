// Minimal lib.rs for testing compilation

// Declare modules
pub mod audio;
pub mod whisper_engine;
pub mod analytics;
pub mod utils;
pub mod tray;

use tauri::{Runtime, AppHandle};
use log::{info, error};

/// Simple test command
#[tauri::command]
async fn test_command() -> Result<String, String> {
    Ok("Hello from simplified app!".to_string())
}

/// Start recording - minimal version
#[tauri::command]
async fn start_recording<R: Runtime>(_app: AppHandle<R>) -> Result<(), String> {
    info!("Test start recording");
    Ok(())
}

/// Stop recording - minimal version
#[tauri::command]
async fn stop_recording<R: Runtime>(_app: AppHandle<R>) -> Result<(), String> {
    info!("Test stop recording");
    Ok(())
}

/// Check if recording is active - minimal version
#[tauri::command]
async fn is_recording() -> bool {
    false
}

pub fn run() {
    tauri::Builder::default()
        .setup(|_app| {
            info!("Minimal audio app initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            test_command,
            start_recording,
            stop_recording,
            is_recording,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}