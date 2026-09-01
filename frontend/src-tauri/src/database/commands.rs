use log::{error, info};
use tauri::{AppHandle, Emitter, Manager};

use super::manager::DatabaseManager;
use crate::state::AppState;

/// Check if this is the first launch (no database exists yet)
#[tauri::command]
pub async fn check_first_launch(app: AppHandle) -> Result<bool, String> {
    DatabaseManager::is_first_launch(&app)
        .await
        .map_err(|e| format!("Failed to check first launch: {}", e))
}

/// Initialize a fresh database (for users who don't want to import)
#[tauri::command]
pub async fn initialize_fresh_database(app: AppHandle) -> Result<(), String> {
    info!("Initializing fresh database");

    let db_manager = DatabaseManager::new_from_app_handle(&app)
        .await
        .map_err(|e| {
            error!("Failed to initialize fresh database: {}", e);
            format!("Failed to initialize database: {}", e)
        })?;

    // Update app state with the new manager
    app.manage(AppState { db_manager: db_manager.clone() });

    // Set default model configuration for fresh installs
    let pool = db_manager.pool();
    
    let default_summary_model = crate::summary::summary_engine::commands::get_recommended_summary_model_for_current_system()
        .unwrap_or("qwen3.5:2b");

    // Default Summary Model: Built-in AI (Qwen recommendation for this system)
    if let Err(e) = crate::database::repositories::setting::SettingsRepository::save_model_config(
        pool,
        "builtin-ai",
        default_summary_model,
        "large-v3", // Default whisper model (unused for builtin but required)
        None,
    ).await {
        error!("Failed to set default summary model config: {}", e);
    }

    // Default Transcription Model: Parakeet
    if let Err(e) = crate::database::repositories::setting::SettingsRepository::save_transcript_config(
        pool,
        "parakeet",
        crate::config::DEFAULT_PARAKEET_MODEL,
    ).await {
        error!("Failed to set default transcription model config: {}", e);
    }

    info!("Fresh database initialized successfully with default models");

    // Emit event to notify frontend that database is ready
    app.emit("database-initialized", ())
        .map_err(|e| format!("Failed to emit database-initialized event: {}", e))?;

    Ok(())
}

/// Get the database directory path
#[tauri::command]
pub async fn get_database_directory(app: AppHandle) -> Result<String, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;

    Ok(app_data_dir.to_string_lossy().to_string())
}

/// Open the database folder in the system file explorer
#[tauri::command]
pub async fn open_database_folder(app: AppHandle) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;

    // Ensure directory exists before trying to open it
    if !app_data_dir.exists() {
        std::fs::create_dir_all(&app_data_dir)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let folder_path = app_data_dir.to_string_lossy().to_string();

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&folder_path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&folder_path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&folder_path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    info!("Opened database folder: {}", folder_path);
    Ok(())
}
