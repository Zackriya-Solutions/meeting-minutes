use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{database::repositories::diarization::DiarizationRepository, state::AppState};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiarizationSettingsDto {
    pub enabled: bool,
    pub mode: String,
    pub show_provisional_labels: bool,
    pub post_call_refinement_enabled: bool,
    pub overlap_handling: String,
    pub speaker_review_enabled: bool,
}

#[tauri::command]
pub async fn get_diarization_settings(
    state: State<'_, AppState>,
) -> Result<DiarizationSettingsDto, String> {
    let settings = DiarizationRepository::get_settings(state.db_manager.pool())
        .await
        .map_err(|error| error.to_string())?;

    Ok(DiarizationSettingsDto {
        enabled: settings.enabled != 0,
        mode: settings.mode,
        show_provisional_labels: settings.show_provisional_labels != 0,
        post_call_refinement_enabled: settings.post_call_refinement_enabled != 0,
        overlap_handling: settings.overlap_handling,
        speaker_review_enabled: settings.speaker_review_enabled != 0,
    })
}

#[tauri::command]
pub async fn save_diarization_settings(
    state: State<'_, AppState>,
    settings: DiarizationSettingsDto,
) -> Result<(), String> {
    DiarizationRepository::save_settings(
        state.db_manager.pool(),
        settings.enabled,
        &settings.mode,
        settings.show_provisional_labels,
        settings.post_call_refinement_enabled,
        &settings.overlap_handling,
        settings.speaker_review_enabled,
    )
    .await
    .map_err(|error| error.to_string())
}
