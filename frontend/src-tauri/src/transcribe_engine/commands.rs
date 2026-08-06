// transcribe_engine/commands.rs
//
// Tauri command surface. This replaces the 24 whisper_*/parakeet_* commands the
// frontend used to call with 12 — the two families were always the same 12
// operations duplicated per engine, and there is only one engine now.
//
// Event names and payloads are deliberately unchanged from the whisper path
// (model-download-progress, model-loading-started, …) so the model manager UI
// keeps working against the same contract.

use crate::config::DEFAULT_TRANSCRIBE_MODEL;
use crate::transcribe_engine::{ModelInfo, ModelStatus, TranscribeEngine};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{command, AppHandle, Emitter, Manager, Runtime};

pub static TRANSCRIBE_ENGINE: Mutex<Option<Arc<TranscribeEngine>>> = Mutex::new(None);

static MODELS_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Called during app setup, before `transcribe_init`.
pub fn set_models_directory<R: Runtime>(app: &AppHandle<R>) {
    let app_data_dir = app.path().app_data_dir().expect("Failed to get app data dir");
    let models_dir = app_data_dir.join("models");

    if !models_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&models_dir) {
            log::error!("Failed to create models directory: {}", e);
            return;
        }
    }
    log::info!("Models directory set to: {}", models_dir.display());

    // Files from the whisper-rs / ONNX engines can never be loaded again, so
    // clear them here rather than leaving GBs of dead weight on disk.
    TranscribeEngine::purge_legacy_models(&models_dir);

    *MODELS_DIR.lock().unwrap() = Some(models_dir);
}

fn get_models_directory() -> Option<PathBuf> {
    MODELS_DIR.lock().unwrap().clone()
}

/// Shared accessor — every command below needs the engine or a uniform error.
fn engine() -> Result<Arc<TranscribeEngine>, String> {
    TRANSCRIBE_ENGINE
        .lock()
        .unwrap()
        .as_ref()
        .cloned()
        .ok_or_else(|| "Transcription engine not initialized".to_string())
}

#[command]
pub async fn transcribe_init() -> Result<(), String> {
    let mut guard = TRANSCRIBE_ENGINE.lock().unwrap();
    if guard.is_some() {
        return Ok(());
    }
    let e = TranscribeEngine::new_with_models_dir(get_models_directory())
        .map_err(|e| format!("Failed to initialize transcription engine: {}", e))?;
    *guard = Some(Arc::new(e));
    Ok(())
}

#[command]
pub async fn transcribe_get_available_models() -> Result<Vec<ModelInfo>, String> {
    // Unlike the whisper path there is no standalone-scan fallback: init is
    // cheap (no model load), so just initialize instead of duplicating the
    // discovery logic for the uninitialized case.
    transcribe_init().await?;
    engine()?
        .discover_models()
        .await
        .map_err(|e| format!("Failed to discover models: {}", e))
}

#[command]
pub async fn transcribe_has_available_models() -> Result<bool, String> {
    Ok(transcribe_get_available_models()
        .await?
        .iter()
        .any(|m| matches!(m.status, ModelStatus::Available)))
}

#[command]
pub async fn transcribe_get_models_directory() -> Result<String, String> {
    transcribe_init().await?;
    Ok(engine()?.get_models_directory().await.display().to_string())
}

#[command]
pub async fn transcribe_load_model<R: Runtime>(
    app_handle: AppHandle<R>,
    model_name: String,
) -> Result<(), String> {
    let engine = engine()?;

    let _ = app_handle.emit(
        "model-loading-started",
        serde_json::json!({ "modelName": model_name }),
    );

    let result = engine
        .load_model(&model_name)
        .await
        .map_err(|e| format!("Failed to load model: {}", e));

    match &result {
        Ok(()) => {
            let _ = app_handle.emit(
                "model-loading-completed",
                serde_json::json!({ "modelName": model_name }),
            );
        }
        Err(error) => {
            let _ = app_handle.emit(
                "model-loading-failed",
                serde_json::json!({ "modelName": model_name, "error": error }),
            );
        }
    }
    result
}

#[command]
pub async fn transcribe_is_model_loaded() -> Result<bool, String> {
    Ok(engine()?.is_model_loaded().await)
}

#[command]
pub async fn transcribe_get_current_model() -> Result<Option<String>, String> {
    Ok(engine()?.get_current_model().await)
}

#[command]
pub async fn transcribe_download_model<R: Runtime>(
    app_handle: AppHandle<R>,
    model_name: String,
) -> Result<(), String> {
    transcribe_init().await?;
    let engine = engine()?;

    let app_for_progress = app_handle.clone();
    let name_for_progress = model_name.clone();
    let progress_callback = Box::new(move |progress: u8| {
        let _ = app_for_progress.emit(
            "model-download-progress",
            serde_json::json!({ "modelName": name_for_progress, "progress": progress }),
        );
    });

    match engine.download_model(&model_name, Some(progress_callback)).await {
        Ok(()) => {
            let _ = app_handle.emit(
                "model-download-complete",
                serde_json::json!({ "modelName": model_name }),
            );
            Ok(())
        }
        Err(e) => {
            let _ = app_handle.emit(
                "model-download-error",
                serde_json::json!({ "modelName": model_name, "error": e.to_string() }),
            );
            Err(format!("Failed to download model: {}", e))
        }
    }
}

#[command]
pub async fn transcribe_cancel_download(model_name: String) -> Result<(), String> {
    engine()?
        .cancel_download(&model_name)
        .await
        .map_err(|e| format!("Failed to cancel download: {}", e))
}

/// Deletes the model file. Named for the UI flow that calls it (removing a
/// corrupted download), but it deletes any catalog model.
#[command]
pub async fn transcribe_delete_corrupted_model(model_name: String) -> Result<(), String> {
    transcribe_init().await?;
    engine()?
        .delete_model(&model_name)
        .await
        .map_err(|e| format!("Failed to delete model: {}", e))
}

/// Ensure a model is loaded and ready before recording starts. Loads the
/// configured model, falling back to any downloaded catalog model.
#[command]
pub async fn transcribe_validate_model_ready<R: Runtime>(
    app: AppHandle<R>,
) -> Result<String, String> {
    let stored =
        crate::api::api::api_get_transcript_config(app.clone(), app.state(), None).await;

    // A built-in audio model has no catalog GGUF to load; check that both its
    // files are on disk instead, which is the equivalent failure to catch before
    // the user starts talking.
    if let Ok(Some(config)) = &stored {
        if crate::config::is_builtin_transcript_provider(&config.provider) {
            let model = if config.model.is_empty() {
                crate::config::DEFAULT_BUILTIN_TRANSCRIBE_MODEL.to_string()
            } else {
                config.model.clone()
            };
            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("Could not resolve the app data directory: {}", e))?;

            let weights = crate::summary::summary_engine::models::get_model_path(
                &app_data_dir,
                &model,
            )
            .map_err(|e| e.to_string())?;
            let projector =
                crate::summary::summary_engine::models::get_mmproj_path(&app_data_dir, &model)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("{} cannot transcribe audio", model))?;

            for path in [&weights, &projector] {
                if !path.exists() {
                    return Err(format!(
                        "{} is not downloaded yet — {} is missing",
                        model,
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
            }
            return Ok(model);
        }
    }

    transcribe_init().await?;
    let engine = engine()?;

    let configured = match stored {
        Ok(Some(config)) if !config.model.is_empty() => config.model,
        _ => DEFAULT_TRANSCRIBE_MODEL.to_string(),
    };

    if engine.get_current_model().await.as_deref() == Some(configured.as_str())
        && engine.is_model_loaded().await
    {
        return Ok(configured);
    }

    let models = engine
        .discover_models()
        .await
        .map_err(|e| format!("Failed to discover models: {}", e))?;

    let target = models
        .iter()
        .find(|m| m.name == configured && matches!(m.status, ModelStatus::Available))
        .or_else(|| {
            models
                .iter()
                .find(|m| matches!(m.status, ModelStatus::Available))
        })
        .ok_or_else(|| {
            format!(
                "Model '{}' is not downloaded. Download it from settings first.",
                configured
            )
        })?;

    if target.name != configured {
        log::warn!(
            "Configured model '{}' unavailable, falling back to '{}'",
            configured,
            target.name
        );
    }

    engine
        .load_model(&target.name)
        .await
        .map_err(|e| format!("Failed to load model '{}': {}", target.name, e))?;
    Ok(target.name.clone())
}

/// Reveal the models directory in the OS file manager.
#[command]
pub async fn open_models_folder() -> Result<(), String> {
    let dir = get_models_directory().ok_or_else(|| "Models directory not initialized".to_string())?;

    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "linux")]
    let program = "xdg-open";

    std::process::Command::new(program)
        .arg(&dir)
        .spawn()
        .map_err(|e| format!("Failed to open models folder: {}", e))?;
    Ok(())
}

#[command]
pub async fn transcribe_transcribe_audio(
    audio_data: Vec<f32>,
    language: Option<String>,
) -> Result<String, String> {
    engine()?
        .transcribe_batch(audio_data, language)
        .await
        .map(|r| r.text)
        .map_err(|e| format!("Transcription failed: {}", e))
}

/// The built-in audio LLMs offered for transcription.
///
/// Exposed as a command rather than mirrored in TypeScript so audio-capable models
/// have exactly one definition (`summary_engine::models`). A model qualifies by
/// carrying an audio projector, so this list cannot drift from what can actually
/// transcribe. Download state and progress come from the existing built-in AI flow.
#[derive(serde::Serialize)]
pub struct BuiltinTranscribeModel {
    pub name: String,
    pub size_mb: u64,
    pub description: String,
}

#[command]
pub fn transcribe_builtin_models() -> Vec<BuiltinTranscribeModel> {
    crate::summary::summary_engine::models::get_available_models()
        .into_iter()
        .filter(|m| m.is_audio())
        .map(|m| BuiltinTranscribeModel {
            size_mb: m.total_size_mb(),
            name: m.name,
            description: m.description,
        })
        .collect()
}

/// Language codes the currently loaded model supports.
///
/// `None` when no model is loaded; an empty vec when the model advertises no
/// list, which means it is language-agnostic rather than language-less. This is
/// the authoritative source for the language picker — the catalog's `languages`
/// string is only a pre-download blurb.
#[command]
pub async fn transcribe_model_languages() -> Result<Option<Vec<String>>, String> {
    Ok(engine()?.model_languages().await)
}
