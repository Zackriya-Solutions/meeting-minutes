use super::{
    manager::MeetingDetectionManager,
    types::{MeetingDetectionSettings, MeetingDetectionStatus},
};
use std::sync::Arc;
use tauri::{State, Wry};
use tokio::sync::RwLock;

pub type MeetingDetectionManagerState = Arc<RwLock<Option<MeetingDetectionManager>>>;

pub async fn initialize_meeting_detection_manager(
    app_handle: tauri::AppHandle<Wry>,
) -> anyhow::Result<MeetingDetectionManager> {
    MeetingDetectionManager::new(app_handle).await
}

#[tauri::command]
pub async fn get_meeting_detection_settings(
    manager_state: State<'_, MeetingDetectionManagerState>,
) -> Result<MeetingDetectionSettings, String> {
    let state = manager_state.read().await;
    let manager = state
        .as_ref()
        .ok_or_else(|| "Meeting detection manager not initialized".to_string())?;

    Ok(manager.get_settings().await)
}

#[tauri::command]
pub async fn set_meeting_detection_settings(
    settings: MeetingDetectionSettings,
    manager_state: State<'_, MeetingDetectionManagerState>,
) -> Result<(), String> {
    let state = manager_state.read().await;
    let manager = state
        .as_ref()
        .ok_or_else(|| "Meeting detection manager not initialized".to_string())?;

    manager
        .update_settings(settings)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_meeting_detection_status(
    manager_state: State<'_, MeetingDetectionManagerState>,
) -> Result<MeetingDetectionStatus, String> {
    let state = manager_state.read().await;
    let manager = state
        .as_ref()
        .ok_or_else(|| "Meeting detection manager not initialized".to_string())?;

    Ok(manager.get_status().await)
}

#[tauri::command]
pub async fn start_meeting_detection(
    manager_state: State<'_, MeetingDetectionManagerState>,
) -> Result<(), String> {
    let state = manager_state.read().await;
    let manager = state
        .as_ref()
        .ok_or_else(|| "Meeting detection manager not initialized".to_string())?;

    manager.start().await;
    Ok(())
}

#[tauri::command]
pub async fn stop_meeting_detection(
    manager_state: State<'_, MeetingDetectionManagerState>,
) -> Result<(), String> {
    let state = manager_state.read().await;
    let manager = state
        .as_ref()
        .ok_or_else(|| "Meeting detection manager not initialized".to_string())?;

    manager.stop().await;
    Ok(())
}
