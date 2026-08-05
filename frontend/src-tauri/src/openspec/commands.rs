use crate::openspec::service::{OpenSpecErrorCode, OpenSpecErrorPayload, OpenSpecGenerationResult, OpenSpecService};
use crate::openspec::setup::{self, OpenSpecSetupDecision, OpenSpecSetupStatusPayload};
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

/// Reports whether the OpenSpec CLI (and its Node.js/npm dependency chain) is
/// currently available, plus the persisted user decision (installed /
/// skipped / never asked). Cheap: only does PATH lookups + a store read.
#[tauri::command]
pub async fn check_openspec_setup_status<R: Runtime>(
    app: AppHandle<R>,
) -> Result<OpenSpecSetupStatusPayload, String> {
    // Pick up any previously-installed portable Node.js / OpenSpec before
    // checking, in case this is a fresh process that hasn't re-derived PATH
    // from a prior install yet.
    setup::ensure_local_tools_on_path(&app);

    let decision = setup::load_setup_decision(&app).await;

    Ok(OpenSpecSetupStatusPayload {
        decision,
        node_available: which::which(setup::node_exe_name()).is_ok(),
        npm_available: which::which(setup::npm_exe_name()).is_ok(),
        openspec_available: setup::verify_openspec_functional(&app).await,
    })
}

#[tauri::command]
pub async fn check_node_runtime_status<R: Runtime>(
    app: AppHandle<R>,
) -> Result<setup::NodeRuntimeStatusPayload, String> {
    setup::ensure_local_tools_on_path(&app);
    Ok(setup::node_runtime_status(&app))
}

#[tauri::command]
pub async fn install_node_runtime<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    setup::install_portable_node_runtime(&app).await
}

#[tauri::command]
pub async fn install_openspec_cli<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    setup::install_openspec_cli(&app).await
}

/// Runs the full OpenSpec CLI setup flow (portable Node.js download if
/// needed, then `npm install -g @fission-ai/openspec@latest`), streaming
/// progress via the `openspec-setup-progress` event. On success, persists the
/// "Installed" decision so the setup prompt never appears again.
#[tauri::command]
pub async fn install_openspec_setup<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    setup::install(&app).await
}

/// Persists that the user explicitly dismissed the OpenSpec CLI setup
/// prompt, so it is not shown again on subsequent app launches.
#[tauri::command]
pub async fn skip_openspec_setup<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    setup::save_setup_decision(&app, OpenSpecSetupDecision::Skipped).await
}
