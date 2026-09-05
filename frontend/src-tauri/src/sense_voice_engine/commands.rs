use super::{model::model_definition, ModelStatus, SenseVoiceEngine, SENSE_VOICE_MODEL};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub static SENSE_VOICE_ENGINE: Mutex<Option<Arc<SenseVoiceEngine>>> = Mutex::new(None);
static MODELS_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn set_models_directory<R: Runtime>(app: &AppHandle<R>) {
    let models_dir = app
        .path()
        .app_data_dir()
        .expect("Failed to get app data directory")
        .join("models");
    if let Err(error) = std::fs::create_dir_all(&models_dir) {
        log::error!("Failed to create SenseVoice model directory: {error}");
        return;
    }
    *MODELS_DIR.lock().unwrap() = Some(models_dir);
}

fn engine() -> Result<Arc<SenseVoiceEngine>, String> {
    SENSE_VOICE_ENGINE
        .lock()
        .unwrap()
        .as_ref()
        .cloned()
        .ok_or_else(|| "SenseVoice engine is not initialized".to_string())
}

#[tauri::command]
pub async fn sense_voice_init() -> Result<(), String> {
    let mut guard = SENSE_VOICE_ENGINE.lock().unwrap();
    if guard.is_some() {
        return Ok(());
    }
    let models_dir = MODELS_DIR
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "SenseVoice model directory is not configured".to_string())?;
    *guard = Some(Arc::new(
        SenseVoiceEngine::new(models_dir).map_err(|error| error.to_string())?,
    ));
    Ok(())
}

#[tauri::command]
pub async fn sense_voice_get_available_models() -> Result<Vec<super::ModelInfo>, String> {
    Ok(engine()?.discover_models())
}

#[tauri::command]
pub async fn sense_voice_load_model<R: Runtime>(
    app: AppHandle<R>,
    model_name: String,
) -> Result<(), String> {
    app.emit("sense-voice-model-loading-started", &model_name)
        .map_err(|error| error.to_string())?;
    let result = engine()?
        .load_model(&model_name)
        .await
        .map_err(|error| error.to_string());
    let event = if result.is_ok() {
        "sense-voice-model-loading-completed"
    } else {
        "sense-voice-model-loading-failed"
    };
    let _ = app.emit(
        event,
        serde_json::json!({
            "modelName": model_name,
            "error": result.as_ref().err(),
        }),
    );
    result
}

#[tauri::command]
pub async fn sense_voice_get_current_model() -> Result<Option<String>, String> {
    Ok(engine()?.get_current_model().await)
}

#[tauri::command]
pub async fn sense_voice_is_model_loaded() -> Result<bool, String> {
    Ok(engine()?.is_model_loaded().await)
}

#[tauri::command]
pub async fn sense_voice_validate_model_ready(
    model_name: Option<String>,
) -> Result<String, String> {
    let engine = engine()?;
    let model_name = model_name.unwrap_or_else(|| SENSE_VOICE_MODEL.to_string());
    if model_definition(&model_name).is_none() {
        return Err(format!("Unknown SenseVoice model: {model_name}"));
    }
    if engine.get_current_model().await.as_deref() == Some(model_name.as_str()) {
        return Ok(model_name);
    }
    let model = engine
        .discover_models()
        .into_iter()
        .find(|model| model.name == model_name)
        .ok_or_else(|| format!("Unknown SenseVoice model: {model_name}"))?;
    if model.status != ModelStatus::Available {
        return Err("SenseVoice model is not downloaded. Download it from Transcript Settings before recording.".to_string());
    }
    engine
        .load_model(&model_name)
        .await
        .map_err(|error| error.to_string())?;
    Ok(model_name)
}

#[tauri::command]
pub async fn sense_voice_transcribe_audio(audio_data: Vec<f32>) -> Result<String, String> {
    engine()?
        .transcribe_audio(audio_data)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn sense_voice_download_model<R: Runtime>(
    app: AppHandle<R>,
    model_name: String,
) -> Result<(), String> {
    if model_definition(&model_name).is_none() {
        return Err(format!("Unknown SenseVoice model: {model_name}"));
    }
    let engine = engine()?;
    let progress_app = app.clone();
    let progress_model = model_name.clone();
    let download_model = model_name.clone();
    let result = engine
        .download_model(&download_model, move |progress| {
            let _ = progress_app.emit(
                "sense-voice-model-download-progress",
                serde_json::json!({
                    "modelName": progress_model,
                    "progress": progress.percent,
                    "downloaded_bytes": progress.downloaded_bytes,
                    "total_bytes": progress.total_bytes,
                    "downloaded_mb": progress.downloaded_mb,
                    "total_mb": progress.total_mb,
                    "speed_mbps": progress.speed_mbps,
                }),
            );
        })
        .await;

    match result {
        Ok(()) => {
            let _ = app.emit(
                "sense-voice-model-download-complete",
                serde_json::json!({ "modelName": model_name }),
            );
            Ok(())
        }
        Err(error) => {
            let message = error.to_string();
            let _ = app.emit(
                "sense-voice-model-download-error",
                serde_json::json!({ "modelName": model_name, "error": message }),
            );
            Err(message)
        }
    }
}

#[tauri::command]
pub async fn sense_voice_cancel_download() -> Result<(), String> {
    if engine()?.cancel_download() {
        Ok(())
    } else {
        Err("No SenseVoice model download is running".to_string())
    }
}

#[tauri::command]
pub async fn sense_voice_delete_model(model_name: String) -> Result<(), String> {
    engine()?
        .delete_model(&model_name)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn open_sense_voice_models_folder() -> Result<(), String> {
    let path = engine()?.models_dir().to_path_buf();
    std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "linux")]
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}
