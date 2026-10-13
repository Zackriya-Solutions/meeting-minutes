use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use log::{error, warn};
use serde::Serialize;
use sqlx::SqlitePool;

use crate::database::repositories::meeting::MeetingsRepository;

#[derive(Debug, Default, Serialize)]
pub struct PurgeOutcome {
    pub purged: Vec<String>,
    pub failed: Vec<String>,
}

/// True when `path` resolves (after following symlinks) strictly inside one of `roots`.
pub fn is_within_roots(path: &Path, roots: &[PathBuf]) -> bool {
    let Ok(path) = path.canonicalize() else {
        return false;
    };
    roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| path != root && path.starts_with(&root))
}

/// Permanently delete one trashed meeting: its recording folder first, then its rows.
/// If the folder can't be removed the meeting stays in Trash so the next run retries.
pub async fn purge_meeting(
    pool: &SqlitePool,
    meeting_id: &str,
    roots: &[PathBuf],
) -> Result<(), String> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT folder_path FROM meetings WHERE id = ? AND deleted_at IS NOT NULL")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    let Some((folder_path,)) = row else {
        return Err(format!("Meeting not in Trash: {}", meeting_id));
    };

    if let Some(folder_path) = folder_path {
        remove_recording(pool, folder_path, roots, meeting_id).await?;
    }

    let deleted = MeetingsRepository::delete_trashed_meeting(pool, meeting_id)
        .await
        .map_err(|e| e.to_string())?;
    if !deleted {
        return Err(format!("Meeting is no longer in Trash: {}", meeting_id));
    }
    Ok(())
}

/// Remove a meeting's recording unless another meeting still uses the same folder.
async fn remove_recording(
    pool: &SqlitePool,
    folder_path: String,
    roots: &[PathBuf],
    meeting_id: &str,
) -> Result<(), String> {
    let shared: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM meetings WHERE folder_path = ? AND id != ? LIMIT 1")
            .bind(&folder_path)
            .bind(meeting_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    if shared.is_some() {
        warn!(
            "Trash purge: recording folder is used by another meeting, leaving files; meeting_id={}",
            meeting_id
        );
        return Ok(());
    }

    let (roots, id) = (roots.to_vec(), meeting_id.to_string());
    tokio::task::spawn_blocking(move || remove_recording_files(Path::new(&folder_path), &roots, &id))
        .await
        .map_err(|e| format!("Recording removal task failed; meeting_id={}: {}", meeting_id, e))?
}

/// Blocking filesystem half of `remove_recording`. Missing paths are fine; paths outside
/// the recordings roots are left alone.
fn remove_recording_files(path: &Path, roots: &[PathBuf], meeting_id: &str) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    if !is_within_roots(path, roots) {
        warn!(
            "Trash purge: recording folder is outside the recordings root, leaving files; meeting_id={}",
            meeting_id
        );
        return Ok(());
    }
    let removed = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    removed.map_err(|e| format!("Failed to remove recording folder; meeting_id={}: {}", meeting_id, e))
}

pub async fn purge_many(pool: &SqlitePool, ids: Vec<String>, roots: &[PathBuf]) -> PurgeOutcome {
    let mut outcome = PurgeOutcome::default();
    for id in ids {
        match purge_meeting(pool, &id, roots).await {
            Ok(()) => outcome.purged.push(id),
            Err(e) => {
                error!("Trash purge failed: {}", e);
                outcome.failed.push(id);
            }
        }
    }
    outcome
}

pub async fn expired_meeting_ids(
    pool: &SqlitePool,
    retention_days: u32,
    now: DateTime<Utc>,
) -> Result<Vec<String>, sqlx::Error> {
    if retention_days == 0 {
        return Ok(Vec::new());
    }
    let cutoff = now - Duration::days(retention_days as i64);
    Ok(MeetingsRepository::list_trash(pool)
        .await?
        .into_iter()
        .filter(|m| m.deleted_at.0 <= cutoff)
        .map(|m| m.id)
        .collect())
}

pub async fn purge_expired(
    pool: &SqlitePool,
    roots: &[PathBuf],
    retention_days: u32,
    now: DateTime<Utc>,
) -> Result<PurgeOutcome, sqlx::Error> {
    let ids = expired_meeting_ids(pool, retention_days, now).await?;
    Ok(purge_many(pool, ids, roots).await)
}

/// Whole days until auto-purge; `None` when retention is "never".
pub fn days_left(deleted_at: DateTime<Utc>, retention_days: u32, now: DateTime<Utc>) -> Option<i64> {
    if retention_days == 0 {
        return None;
    }
    let expires = deleted_at + Duration::days(retention_days as i64);
    Some((expires - now).num_days().max(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::meeting::MeetingsRepository;
    use crate::database::test_support::{insert_meeting, test_pool};
    use chrono::TimeZone;

    async fn trash_at(pool: &SqlitePool, id: &str, when: DateTime<Utc>) {
        sqlx::query("UPDATE meetings SET deleted_at = ? WHERE id = ?")
            .bind(when)
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 11, 12, 0, 0).unwrap()
    }

    #[tokio::test]
    async fn expired_respects_retention_and_never() {
        let pool = test_pool().await;
        insert_meeting(&pool, "meeting-old", "old", None).await;
        insert_meeting(&pool, "meeting-new", "new", None).await;
        trash_at(&pool, "meeting-old", now() - Duration::days(31)).await;
        trash_at(&pool, "meeting-new", now() - Duration::days(2)).await;

        assert_eq!(expired_meeting_ids(&pool, 30, now()).await.unwrap(), vec!["meeting-old"]);
        assert!(expired_meeting_ids(&pool, 0, now()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn purge_removes_folder_inside_root_then_rows() {
        let pool = test_pool().await;
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("Standup_2026");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("audio.mp4"), b"x").unwrap();
        insert_meeting(&pool, "meeting-1", "Standup", Some(folder.to_str().unwrap())).await;
        trash_at(&pool, "meeting-1", now() - Duration::days(40)).await;

        let outcome = purge_expired(&pool, &[root.path().to_path_buf()], 30, now()).await.unwrap();
        assert_eq!(outcome.purged, vec!["meeting-1"]);
        assert!(!folder.exists());
        assert!(MeetingsRepository::list_trash(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn purge_keeps_files_outside_root_but_deletes_rows() {
        let pool = test_pool().await;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        insert_meeting(&pool, "meeting-1", "A", Some(outside.path().to_str().unwrap())).await;
        trash_at(&pool, "meeting-1", now()).await;

        purge_meeting(&pool, "meeting-1", &[root.path().to_path_buf()]).await.unwrap();
        assert!(outside.path().exists());
        assert!(MeetingsRepository::list_trash(&pool).await.unwrap().is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn purge_does_not_follow_symlink_out_of_root() {
        let pool = test_pool().await;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("keep.txt"), b"x").unwrap();
        let link = root.path().join("linked");
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        insert_meeting(&pool, "meeting-1", "A", Some(link.to_str().unwrap())).await;
        trash_at(&pool, "meeting-1", now()).await;

        purge_meeting(&pool, "meeting-1", &[root.path().to_path_buf()]).await.unwrap();
        assert!(outside.path().join("keep.txt").exists());
    }

    #[tokio::test]
    async fn purge_tolerates_missing_folder_and_rejects_live_meeting() {
        let pool = test_pool().await;
        let root = tempfile::tempdir().unwrap();
        let gone = root.path().join("gone");
        insert_meeting(&pool, "meeting-1", "A", Some(gone.to_str().unwrap())).await;
        insert_meeting(&pool, "meeting-2", "B", None).await;
        trash_at(&pool, "meeting-1", now()).await;

        purge_meeting(&pool, "meeting-1", &[root.path().to_path_buf()]).await.unwrap();
        assert!(purge_meeting(&pool, "meeting-2", &[root.path().to_path_buf()]).await.is_err());
        assert_eq!(MeetingsRepository::get_meetings(&pool).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn purge_keeps_folder_shared_with_another_meeting() {
        let pool = test_pool().await;
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("Import_2026-10-11_12-00");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("audio.mp4"), b"x").unwrap();
        let fp = folder.to_str().unwrap();
        insert_meeting(&pool, "meeting-1", "A", Some(fp)).await;
        insert_meeting(&pool, "meeting-2", "A", Some(fp)).await;
        trash_at(&pool, "meeting-1", now()).await;

        purge_meeting(&pool, "meeting-1", &[root.path().to_path_buf()]).await.unwrap();
        assert!(folder.join("audio.mp4").exists());
        assert!(MeetingsRepository::list_trash(&pool).await.unwrap().is_empty());
        let left = MeetingsRepository::get_meetings(&pool).await.unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, "meeting-2");
    }

    #[tokio::test]
    async fn delete_trashed_meeting_refuses_live_meeting() {
        let pool = test_pool().await;
        insert_meeting(&pool, "meeting-1", "A", None).await;
        assert!(!MeetingsRepository::delete_trashed_meeting(&pool, "meeting-1").await.unwrap());
        assert_eq!(MeetingsRepository::get_meetings(&pool).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn purge_removes_folder_path_that_is_a_file() {
        let pool = test_pool().await;
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("audio.mp4");
        std::fs::write(&file, b"x").unwrap();
        insert_meeting(&pool, "meeting-1", "A", Some(file.to_str().unwrap())).await;
        trash_at(&pool, "meeting-1", now()).await;

        purge_meeting(&pool, "meeting-1", &[root.path().to_path_buf()]).await.unwrap();
        assert!(!file.exists());
        assert!(MeetingsRepository::list_trash(&pool).await.unwrap().is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn purge_keeps_meeting_in_trash_when_removal_fails() {
        use std::os::unix::fs::PermissionsExt;
        let pool = test_pool().await;
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("parent");
        let folder = parent.join("rec");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("audio.mp4"), b"x").unwrap();
        insert_meeting(&pool, "meeting-1", "A", Some(folder.to_str().unwrap())).await;
        trash_at(&pool, "meeting-1", now()).await;

        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = purge_meeting(&pool, "meeting-1", &[root.path().to_path_buf()]).await;
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();

        // Running as root bypasses permission checks; only assert when removal really failed.
        if folder.exists() {
            assert!(result.is_err());
            assert_eq!(MeetingsRepository::list_trash(&pool).await.unwrap().len(), 1);
        }
    }

    #[test]
    fn root_itself_is_not_within_roots() {
        let root = tempfile::tempdir().unwrap();
        assert!(!is_within_roots(root.path(), &[root.path().to_path_buf()]));
    }

    #[test]
    fn days_left_counts_down_and_is_none_for_never() {
        let deleted = now() - Duration::days(10);
        assert_eq!(days_left(deleted, 30, now()), Some(20));
        assert_eq!(days_left(now() - Duration::days(45), 30, now()), Some(0));
        assert_eq!(days_left(deleted, 0, now()), None);
    }
}
