use chrono::Utc;
use tauri::{AppHandle, Runtime, State};

use super::notify_meetings_changed;
use super::purge::{purge_many, purge_meeting, PurgeOutcome};
use super::scheduler::{read_retention_days, recordings_roots, trash_items, TrashItem};
use crate::database::models::{Folder, Tag, TagWithCount};
use crate::database::repositories::{
    folder::FolderRepository, meeting::MeetingsRepository, org_error::OrgError,
    tag::TagRepository,
};
use crate::state::AppState;

#[tauri::command]
pub async fn list_folders(state: State<'_, AppState>) -> Result<Vec<Folder>, OrgError> {
    FolderRepository::list(state.db_manager.pool()).await
}

#[tauri::command]
pub async fn create_folder<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    name: String,
) -> Result<Folder, OrgError> {
    FolderRepository::create(state.db_manager.pool(), &name)
        .await
        .inspect(|_| notify_meetings_changed(&app))
}

#[tauri::command]
pub async fn rename_folder<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<Folder, OrgError> {
    FolderRepository::rename(state.db_manager.pool(), &id, &name)
        .await
        .inspect(|_| notify_meetings_changed(&app))
}

#[tauri::command]
pub async fn delete_folder<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
) -> Result<u64, OrgError> {
    FolderRepository::delete(state.db_manager.pool(), &id)
        .await
        .inspect(|_| notify_meetings_changed(&app))
}

#[tauri::command]
pub async fn list_tags(state: State<'_, AppState>) -> Result<Vec<TagWithCount>, OrgError> {
    TagRepository::list_with_counts(state.db_manager.pool()).await
}

#[tauri::command]
pub async fn create_tag<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    name: String,
) -> Result<Tag, OrgError> {
    TagRepository::create_or_get(state.db_manager.pool(), &name)
        .await
        .inspect(|_| notify_meetings_changed(&app))
}

#[tauri::command]
pub async fn rename_tag<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<Tag, OrgError> {
    TagRepository::rename(state.db_manager.pool(), &id, &name)
        .await
        .inspect(|_| notify_meetings_changed(&app))
}

#[tauri::command]
pub async fn delete_tag<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), OrgError> {
    TagRepository::delete(state.db_manager.pool(), &id)
        .await
        .inspect(|_| notify_meetings_changed(&app))
}

#[tauri::command]
pub async fn set_meeting_folder<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    meeting_id: String,
    folder_id: Option<String>,
) -> Result<(), OrgError> {
    MeetingsRepository::set_folder(state.db_manager.pool(), &meeting_id, folder_id.as_deref())
        .await
        .inspect(|_| notify_meetings_changed(&app))
}

#[tauri::command]
pub async fn set_meeting_tags<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    meeting_id: String,
    tag_ids: Vec<String>,
) -> Result<Vec<String>, OrgError> {
    TagRepository::set_meeting_tags(state.db_manager.pool(), &meeting_id, &tag_ids)
        .await
        .inspect(|_| notify_meetings_changed(&app))
}

#[tauri::command]
pub async fn trash_meeting<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<(), OrgError> {
    MeetingsRepository::trash(state.db_manager.pool(), &meeting_id)
        .await
        .inspect(|_| notify_meetings_changed(&app))
}

#[tauri::command]
pub async fn restore_meeting<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<(), OrgError> {
    MeetingsRepository::restore(state.db_manager.pool(), &meeting_id)
        .await
        .inspect(|_| notify_meetings_changed(&app))
}

#[tauri::command]
pub async fn delete_meeting_forever<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<(), String> {
    let roots = recordings_roots(&app).await;
    purge_meeting(state.db_manager.pool(), &meeting_id, &roots)
        .await
        .inspect(|_| notify_meetings_changed(&app))
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
    let outcome = purge_many(pool, ids, &roots).await;
    notify_meetings_changed(&app);
    Ok(outcome)
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
