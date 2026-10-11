use chrono::Utc;
use tauri::{AppHandle, Runtime, State};

use super::purge::{purge_many, purge_meeting, PurgeOutcome};
use super::scheduler::{read_retention_days, recordings_roots, trash_items, TrashItem};
use crate::database::models::{Folder, Tag, TagWithCount};
use crate::database::repositories::{
    folder::FolderRepository, meeting::MeetingsRepository, tag::TagRepository,
};
use crate::state::AppState;

#[tauri::command]
pub async fn list_folders(state: State<'_, AppState>) -> Result<Vec<Folder>, String> {
    FolderRepository::list(state.db_manager.pool()).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_folder(state: State<'_, AppState>, name: String) -> Result<Folder, String> {
    FolderRepository::create(state.db_manager.pool(), &name).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rename_folder(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<Folder, String> {
    FolderRepository::rename(state.db_manager.pool(), &id, &name).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_folder(state: State<'_, AppState>, id: String) -> Result<u64, String> {
    FolderRepository::delete(state.db_manager.pool(), &id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_tags(state: State<'_, AppState>) -> Result<Vec<TagWithCount>, String> {
    TagRepository::list_with_counts(state.db_manager.pool()).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_tag(state: State<'_, AppState>, name: String) -> Result<Tag, String> {
    TagRepository::create_or_get(state.db_manager.pool(), &name).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rename_tag(state: State<'_, AppState>, id: String, name: String) -> Result<Tag, String> {
    TagRepository::rename(state.db_manager.pool(), &id, &name).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_tag(state: State<'_, AppState>, id: String) -> Result<(), String> {
    TagRepository::delete(state.db_manager.pool(), &id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_meeting_folder(
    state: State<'_, AppState>,
    meeting_id: String,
    folder_id: Option<String>,
) -> Result<(), String> {
    MeetingsRepository::set_folder(state.db_manager.pool(), &meeting_id, folder_id.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_meeting_tags(
    state: State<'_, AppState>,
    meeting_id: String,
    tag_ids: Vec<String>,
) -> Result<Vec<String>, String> {
    TagRepository::set_meeting_tags(state.db_manager.pool(), &meeting_id, &tag_ids)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn trash_meeting(state: State<'_, AppState>, meeting_id: String) -> Result<(), String> {
    MeetingsRepository::trash(state.db_manager.pool(), &meeting_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn restore_meeting(state: State<'_, AppState>, meeting_id: String) -> Result<(), String> {
    MeetingsRepository::restore(state.db_manager.pool(), &meeting_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_meeting_forever<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<(), String> {
    let roots = recordings_roots(&app).await;
    purge_meeting(state.db_manager.pool(), &meeting_id, &roots).await
}

#[tauri::command]
pub async fn empty_trash<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<PurgeOutcome, String> {
    let pool = state.db_manager.pool();
    let ids = MeetingsRepository::list_trash(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|m| m.id)
        .collect();
    let roots = recordings_roots(&app).await;
    Ok(purge_many(pool, ids, &roots).await)
}

#[tauri::command]
pub async fn list_trash<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<Vec<TrashItem>, String> {
    let trashed = MeetingsRepository::list_trash(state.db_manager.pool())
        .await
        .map_err(|e| e.to_string())?;
    Ok(trash_items(trashed, read_retention_days(&app), Utc::now()))
}
