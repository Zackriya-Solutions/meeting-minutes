use std::path::PathBuf;

use chrono::{DateTime, Utc};
use log::{error, info, warn};
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_store::StoreExt;

use super::notify_meetings_changed;
use super::purge::{days_left, purge_expired};
use crate::audio::recording_preferences::{get_default_recordings_folder, load_recording_preferences};
use crate::database::models::TrashedMeeting;
use crate::state::AppState;

pub const DEFAULT_RETENTION_DAYS: u32 = 30;
const PURGE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Serialize)]
pub struct TrashItem {
    pub id: String,
    pub title: String,
    pub deleted_at: String,
    pub days_left: Option<i64>,
}

pub fn trash_items(trashed: Vec<TrashedMeeting>, retention_days: u32, now: DateTime<Utc>) -> Vec<TrashItem> {
    trashed
        .into_iter()
        .map(|m| TrashItem {
            days_left: days_left(m.deleted_at.0, retention_days, now),
            deleted_at: m.deleted_at.0.to_rfc3339(),
            id: m.id,
            title: m.title,
        })
        .collect()
}

/// `trash_retention_days` from preferences.json; 0 means never auto-purge.
pub fn read_retention_days<R: Runtime>(app: &AppHandle<R>) -> u32 {
    match app.store("preferences.json") {
        Ok(store) => store
            .get("trash_retention_days")
            .and_then(|v| v.as_u64())
            .map(|d| d as u32)
            .unwrap_or(DEFAULT_RETENTION_DAYS),
        Err(e) => {
            warn!("Failed to open preferences store, using default trash retention: {}", e);
            DEFAULT_RETENTION_DAYS
        }
    }
}

/// Directories a purge may delete recording folders from.
pub async fn recordings_roots<R: Runtime>(app: &AppHandle<R>) -> Vec<PathBuf> {
    let mut roots = vec![get_default_recordings_folder()];
    if let Ok(prefs) = load_recording_preferences(app).await {
        if !roots.contains(&prefs.save_folder) {
            roots.push(prefs.save_folder);
        }
    }
    roots
}

/// Purge expired Trash now and then every 24 hours.
pub fn spawn_purge_scheduler<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(PURGE_INTERVAL);
        loop {
            interval.tick().await; // first tick completes immediately
            run_scheduled_purge(&app).await;
        }
    });
}

async fn run_scheduled_purge<R: Runtime>(app: &AppHandle<R>) {
    // AppState is only managed once the database exists (after onboarding on first launch).
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let pool = state.db_manager.pool().clone();
    let roots = recordings_roots(app).await;
    match purge_expired(&pool, &roots, read_retention_days(app), Utc::now()).await {
        Ok(outcome) => {
            if !outcome.purged.is_empty() {
                info!("Trash purge removed {} meeting(s)", outcome.purged.len());
                notify_meetings_changed(app);
            }
            if !outcome.failed.is_empty() {
                warn!("Trash purge left {} meeting(s) in Trash after errors", outcome.failed.len());
            }
        }
        Err(e) => error!("Trash purge failed: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::models::{DateTimeUtc, TrashedMeeting};
    use chrono::{Duration, TimeZone};

    #[test]
    fn trash_items_carry_days_left() {
        let now = Utc.with_ymd_and_hms(2026, 10, 11, 0, 0, 0).unwrap();
        let trashed = vec![TrashedMeeting {
            id: "meeting-1".into(),
            title: "A".into(),
            folder_path: None,
            deleted_at: DateTimeUtc(now - Duration::days(5)),
        }];
        let items = trash_items(trashed, 30, now);
        assert_eq!(items[0].id, "meeting-1");
        assert_eq!(items[0].days_left, Some(25));
        assert!(items[0].deleted_at.starts_with("2026-10-06"));
    }
}
