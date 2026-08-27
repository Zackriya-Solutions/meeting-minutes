use super::models::{self, SpeakerModelStatus};
use tauri::{AppHandle, Runtime};

#[tauri::command]
pub async fn speaker_diarization_get_status<R: Runtime>(
    app: AppHandle<R>,
) -> Result<SpeakerModelStatus, String> {
    models::get_status(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn speaker_diarization_download_model<R: Runtime>(
    app: AppHandle<R>,
) -> Result<(), String> {
    models::download_model(app)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn speaker_diarization_delete_model<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    models::delete_model(&app)
        .await
        .map_err(|error| error.to_string())
}
