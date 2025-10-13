use crate::whisper_engine::{ModelInfo, WhisperEngine};
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use tauri::{command, Emitter, Manager, AppHandle, Runtime};

// Global whisper engine
pub static WHISPER_ENGINE: Mutex<Option<Arc<WhisperEngine>>> = Mutex::new(None);

// Global models directory path (set during app initialization)
static MODELS_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Initialize the models directory path using app_data_dir
/// This should be called during app setup before whisper_init
pub fn set_models_directory<R: Runtime>(app: &AppHandle<R>) {
    let app_data_dir = app.path().app_data_dir()
        .expect("Failed to get app data dir");

    let models_dir = app_data_dir.join("models");

    // Create directory if it doesn't exist
    if !models_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&models_dir) {
            log::error!("Failed to create models directory: {}", e);
            return;
        }
    }

    log::info!("Models directory set to: {}", models_dir.display());

    let mut guard = MODELS_DIR.lock().unwrap();
    *guard = Some(models_dir);
}

/// Get the configured models directory
fn get_models_directory() -> Option<PathBuf> {
    MODELS_DIR.lock().unwrap().clone()
}

#[command]
pub async fn whisper_init() -> Result<(), String> {
    let mut guard = WHISPER_ENGINE.lock().unwrap();
    if guard.is_some() {
        return Ok(());
    }

    let models_dir = get_models_directory();
    let engine = WhisperEngine::new_with_models_dir(models_dir)
        .map_err(|e| format!("Failed to initialize whisper engine: {}", e))?;
    *guard = Some(Arc::new(engine));
    Ok(())
}

#[command]
pub async fn whisper_get_available_models() -> Result<Vec<ModelInfo>, String> {
    let engine = {
        let guard = WHISPER_ENGINE.lock().unwrap();
        guard.as_ref().cloned()
    };

    if let Some(engine) = engine {
        engine
            .discover_models()
            .await
            .map_err(|e| format!("Failed to discover models: {}", e))
    } else {
        Err("Whisper engine not initialized".to_string())
    }
}

#[command]
pub async fn whisper_load_model(
    app_handle: tauri::AppHandle,
    model_name: String
) -> Result<(), String> {
    let engine = {
        let guard = WHISPER_ENGINE.lock().unwrap();
        guard.as_ref().cloned()
    };

    if let Some(engine) = engine {
        // FIX 6: Emit model loading started event
        if let Err(e) = app_handle.emit(
            "model-loading-started",
            serde_json::json!({
                "modelName": model_name
            }),
        ) {
            log::error!("Failed to emit model-loading-started event: {}", e);
        }

        let result = engine
            .load_model(&model_name)
            .await
            .map_err(|e| format!("Failed to load model: {}", e));

        // FIX 6: Emit model loading completed/failed event
        if result.is_ok() {
            if let Err(e) = app_handle.emit(
                "model-loading-completed",
                serde_json::json!({
                    "modelName": model_name
                }),
            ) {
                log::error!("Failed to emit model-loading-completed event: {}", e);
            }
        } else if let Err(ref error) = result {
            if let Err(e) = app_handle.emit(
                "model-loading-failed",
                serde_json::json!({
                    "modelName": model_name,
                    "error": error
                }),
            ) {
                log::error!("Failed to emit model-loading-failed event: {}", e);
            }
        }

        result
    } else {
        Err("Whisper engine not initialized".to_string())
    }
}

#[command]
pub async fn whisper_get_current_model() -> Result<Option<String>, String> {
    let engine = {
        let guard = WHISPER_ENGINE.lock().unwrap();
        guard.as_ref().cloned()
    };

    if let Some(engine) = engine {
        Ok(engine.get_current_model().await)
    } else {
        Err("Whisper engine not initialized".to_string())
    }
}

#[command]
pub async fn whisper_is_model_loaded() -> Result<bool, String> {
    let engine = {
        let guard = WHISPER_ENGINE.lock().unwrap();
        guard.as_ref().cloned()
    };

    if let Some(engine) = engine {
        Ok(engine.is_model_loaded().await)
    } else {
        Err("Whisper engine not initialized".to_string())
    }
}

#[command]
pub async fn whisper_has_available_models() -> Result<bool, String> {
    let engine = {
        let guard = WHISPER_ENGINE.lock().unwrap();
        guard.as_ref().cloned()
    };

    if let Some(engine) = engine {
        let models = engine
            .discover_models()
            .await
            .map_err(|e| format!("Failed to discover models: {}", e))?;

        // Check if at least one model is available
        let available_models: Vec<_> = models
            .iter()
            .filter(|model| matches!(model.status, crate::whisper_engine::ModelStatus::Available))
            .collect();

        Ok(!available_models.is_empty())
    } else {
        Ok(false)
    }
}

#[command]
pub async fn whisper_validate_model_ready() -> Result<String, String> {
    let engine = {
        let guard = WHISPER_ENGINE.lock().unwrap();
        guard.as_ref().cloned()
    };

    if let Some(engine) = engine {
        // Check if a model is currently loaded
        if engine.is_model_loaded().await {
            if let Some(current_model) = engine.get_current_model().await {
                return Ok(current_model);
            }
        }

        // No model loaded, check if any models are available to load
        let models = engine
            .discover_models()
            .await
            .map_err(|e| format!("Failed to discover models: {}", e))?;

        let available_models: Vec<_> = models
            .iter()
            .filter(|model| matches!(model.status, crate::whisper_engine::ModelStatus::Available))
            .collect();

        if available_models.is_empty() {
            return Err(
                "No Whisper models are available. Please download a model to enable transcription."
                    .to_string(),
            );
        }

        // Try to load the first available model
        let first_model = &available_models[0];
        engine
            .load_model(&first_model.name)
            .await
            .map_err(|e| format!("Failed to load model {}: {}", first_model.name, e))?;

        Ok(first_model.name.clone())
    } else {
        Err("Whisper engine not initialized".to_string())
    }
}

/// Internal version of whisper_validate_model_ready that respects user's transcript config
pub async fn whisper_validate_model_ready_with_config<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<String, String> {
    let engine = {
        let guard = WHISPER_ENGINE.lock().unwrap();
        guard.as_ref().cloned()
    };

    if let Some(engine) = engine {
        // Check if a model is currently loaded
        if engine.is_model_loaded().await {
            if let Some(current_model) = engine.get_current_model().await {
                log::info!("Model already loaded: {}", current_model);
                return Ok(current_model);
            }
        }

        // No model loaded - try to load user's configured model from transcript config
        let model_to_load = match crate::api::api::api_get_transcript_config(
            app.clone(),
            app.state(),
            None,
        )
        .await
        {
            Ok(Some(config)) => {
                log::info!(
                    "Got transcript config from API - provider: {}, model: {}",
                    config.provider,
                    config.model
                );
                if config.provider == "localWhisper" && !config.model.is_empty() {
                    log::info!("Using user's configured model: {}", config.model);
                    Some(config.model)
                } else {
                    log::info!(
                        "API config uses non-local provider ({}) or empty model, will auto-select",
                        config.provider
                    );
                    None
                }
            }
            Ok(None) => {
                log::info!("No transcript config found in API, will auto-select model");
                None
            }
            Err(e) => {
                log::warn!(
                    "Failed to get transcript config from API: {}, will auto-select model",
                    e
                );
                None
            }
        };

        // Check available models
        let models = engine
            .discover_models()
            .await
            .map_err(|e| format!("Failed to discover models: {}", e))?;

        let available_models: Vec<_> = models
            .iter()
            .filter(|model| matches!(model.status, crate::whisper_engine::ModelStatus::Available))
            .collect();

        if available_models.is_empty() {
            return Err(
                "No Whisper models are available. Please download a model to enable transcription."
                    .to_string(),
            );
        }

        // Try to load user's configured model if specified
        let model_name = if let Some(configured_model) = model_to_load {
            // Check if configured model is available
            if available_models.iter().any(|m| m.name == configured_model) {
                log::info!("Loading user's configured model: {}", configured_model);
                configured_model
            } else {
                log::warn!(
                    "Configured model '{}' not found, falling back to first available: {}",
                    configured_model,
                    available_models[0].name
                );
                available_models[0].name.clone()
            }
        } else {
            // No configured model, use first available
            log::info!(
                "No configured model, loading first available: {}",
                available_models[0].name
            );
            available_models[0].name.clone()
        };

        engine
            .load_model(&model_name)
            .await
            .map_err(|e| format!("Failed to load model {}: {}", model_name, e))?;

        Ok(model_name)
    } else {
        Err("Whisper engine not initialized".to_string())
    }
}

#[command]
pub async fn whisper_transcribe_audio(audio_data: Vec<f32>) -> Result<String, String> {
    let engine = {
        let guard = WHISPER_ENGINE.lock().unwrap();
        guard.as_ref().cloned()
    };

    if let Some(engine) = engine {
        // Get language preference
        let language = crate::get_language_preference_internal();
        engine
            .transcribe_audio(audio_data, language)
            .await
            .map_err(|e| format!("Transcription failed: {}", e))
    } else {
        Err("Whisper engine not initialized".to_string())
    }
}

#[command]
pub async fn whisper_get_models_directory() -> Result<String, String> {
    let engine = {
        let guard = WHISPER_ENGINE.lock().unwrap();
        guard.as_ref().cloned()
    };

    if let Some(engine) = engine {
        let path = engine.get_models_directory().await;
        Ok(path.to_string_lossy().to_string())
    } else {
        Err("Whisper engine not initialized".to_string())
    }
}

#[command]
pub async fn whisper_download_model(
    app_handle: tauri::AppHandle,
    model_name: String,
) -> Result<(), String> {
    let engine = {
        let guard = WHISPER_ENGINE.lock().unwrap();
        guard.as_ref().cloned()
    };

    if let Some(engine) = engine {
        // Create progress callback that emits events
        let app_handle_clone = app_handle.clone();
        let model_name_clone = model_name.clone();

        let progress_callback = Box::new(move |progress: u8| {
            log::info!("Download progress for {}: {}%", model_name_clone, progress);

            // Emit download progress event
            if let Err(e) = app_handle_clone.emit(
                "model-download-progress",
                serde_json::json!({
                    "modelName": model_name_clone,
                    "progress": progress
                }),
            ) {
                log::error!("Failed to emit download progress event: {}", e);
            }
        });

        let result = engine
            .download_model(&model_name, Some(progress_callback))
            .await;

        match result {
            Ok(()) => {
                // Emit completion event
                if let Err(e) = app_handle.emit(
                    "model-download-complete",
                    serde_json::json!({
                        "modelName": model_name
                    }),
                ) {
                    log::error!("Failed to emit download complete event: {}", e);
                }
                Ok(())
            }
            Err(e) => {
                // Emit error event
                if let Err(emit_e) = app_handle.emit(
                    "model-download-error",
                    serde_json::json!({
                        "modelName": model_name,
                        "error": e.to_string()
                    }),
                ) {
                    log::error!("Failed to emit download error event: {}", emit_e);
                }
                Err(format!("Failed to download model: {}", e))
            }
        }
    } else {
        Err("Whisper engine not initialized".to_string())
    }
}

#[command]
pub async fn whisper_cancel_download(model_name: String) -> Result<(), String> {
    let engine = {
        let guard = WHISPER_ENGINE.lock().unwrap();
        guard.as_ref().cloned()
    };

    if let Some(engine) = engine {
        engine
            .cancel_download(&model_name)
            .await
            .map_err(|e| format!("Failed to cancel download: {}", e))
    } else {
        Err("Whisper engine not initialized".to_string())
    }
}

#[command]
pub async fn whisper_delete_corrupted_model(model_name: String) -> Result<String, String> {
    let engine = {
        let guard = WHISPER_ENGINE.lock().unwrap();
        guard.as_ref().cloned()
    };

    if let Some(engine) = engine {
        engine
            .delete_model(&model_name)
            .await
            .map_err(|e| format!("Failed to delete model: {}", e))
    } else {
        Err("Whisper engine not initialized".to_string())
    }
}

/// Open the models folder in the system file explorer
#[command]
pub async fn open_models_folder() -> Result<(), String> {
    let models_dir = get_models_directory()
        .ok_or_else(|| "Models directory not initialized".to_string())?;

    // Ensure directory exists before trying to open it
    if !models_dir.exists() {
        std::fs::create_dir_all(&models_dir)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let folder_path = models_dir.to_string_lossy().to_string();

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

    log::info!("Opened models folder: {}", folder_path);
    Ok(())
}
