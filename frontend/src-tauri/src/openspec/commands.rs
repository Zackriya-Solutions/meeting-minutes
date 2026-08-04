use crate::openspec::service::{OpenSpecErrorCode, OpenSpecErrorPayload, OpenSpecGenerationResult, OpenSpecService};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::fs;
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::DialogExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveOpenSpecBundleAsResult {
    pub cancelled: bool,
    pub saved_path: Option<String>,
}

#[tauri::command]
pub async fn api_generate_openspec_bundle<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<OpenSpecGenerationResult, OpenSpecErrorPayload> {
    Ok(OpenSpecService::generate_bundle(&app, state.db_manager.pool(), meeting_id).await)
}

#[tauri::command]
pub async fn api_save_openspec_bundle_as<R: Runtime>(
    app: AppHandle<R>,
    zip_temp_path: String,
    suggested_filename: String,
) -> Result<SaveOpenSpecBundleAsResult, OpenSpecErrorPayload> {
    let source = std::path::PathBuf::from(&zip_temp_path);
    if !source.exists() {
        return Err(OpenSpecErrorPayload {
            code: OpenSpecErrorCode::InvalidInput,
            message: "Generated OpenSpec zip file no longer exists".to_string(),
            stderr: Some(zip_temp_path),
        });
    }

    let file_path = app
        .dialog()
        .file()
        .set_file_name(&suggested_filename)
        .add_filter("Zip", &["zip"])
        .blocking_save_file();

    let Some(target) = file_path else {
        return Ok(SaveOpenSpecBundleAsResult {
            cancelled: true,
            saved_path: None,
        });
    };

    let target_path = std::path::PathBuf::from(target.to_string());
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|err| OpenSpecErrorPayload {
            code: OpenSpecErrorCode::IoFailure,
            message: "Failed to prepare target directory for OpenSpec zip".to_string(),
            stderr: Some(err.to_string()),
        })?;
    }

    fs::copy(&source, &target_path).map_err(|err| OpenSpecErrorPayload {
        code: OpenSpecErrorCode::IoFailure,
        message: "Failed to save OpenSpec zip file".to_string(),
        stderr: Some(err.to_string()),
    })?;

    Ok(SaveOpenSpecBundleAsResult {
        cancelled: false,
        saved_path: Some(target_path.to_string_lossy().to_string()),
    })
}
