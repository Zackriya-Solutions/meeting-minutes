use crate::api::{MeetingDetails, MeetingTranscript};
use super::folder::FolderRepository;
use super::org_error::OrgError;
use crate::database::models::{MeetingModel, Transcript, TrashedMeeting};
use chrono::Utc;
use sqlx::{Connection, Error as SqlxError, SqliteConnection, SqlitePool};
use log::{error, info};

pub struct MeetingsRepository;

impl MeetingsRepository {
    pub async fn get_meetings(pool: &SqlitePool) -> Result<Vec<MeetingModel>, sqlx::Error> {
        let meetings =
            sqlx::query_as::<_, MeetingModel>(
                "SELECT * FROM meetings WHERE deleted_at IS NULL ORDER BY created_at DESC",
            )
            .fetch_all(pool)
            .await?;
        Ok(meetings)
    }

    pub async fn set_folder(
        pool: &SqlitePool,
        meeting_id: &str,
        folder_id: Option<&str>,
    ) -> Result<(), OrgError> {
        if let Some(folder_id) = folder_id {
            FolderRepository::get_live(pool, folder_id).await?;
        }
        let result = sqlx::query("UPDATE meetings SET folder_id = ?, updated_at = ? WHERE id = ?")
            .bind(folder_id)
            .bind(Utc::now())
            .bind(meeting_id)
            .execute(pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(OrgError::NotFound(format!("Meeting not found: {}", meeting_id)));
        }
        Ok(())
    }

    pub async fn trash(pool: &SqlitePool, meeting_id: &str) -> Result<(), OrgError> {
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE meetings SET deleted_at = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(now)
        .bind(now)
        .bind(meeting_id)
        .execute(pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(OrgError::NotFound(format!(
                "Meeting not found or already in Trash: {}",
                meeting_id
            )));
        }
        Ok(())
    }

    /// Bring a meeting back from Trash. If its folder was deleted meanwhile, it lands in Unfiled.
    pub async fn restore(pool: &SqlitePool, meeting_id: &str) -> Result<(), OrgError> {
        let result = sqlx::query(
            "UPDATE meetings
             SET deleted_at = NULL,
                 updated_at = ?,
                 folder_id = CASE
                     WHEN folder_id IN (SELECT id FROM folders WHERE deleted_at IS NULL) THEN folder_id
                     ELSE NULL
                 END
             WHERE id = ? AND deleted_at IS NOT NULL",
        )
        .bind(Utc::now())
        .bind(meeting_id)
        .execute(pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(OrgError::NotFound(format!("Meeting not in Trash: {}", meeting_id)));
        }
        Ok(())
    }

    pub async fn list_trash(pool: &SqlitePool) -> Result<Vec<TrashedMeeting>, SqlxError> {
        sqlx::query_as::<_, TrashedMeeting>(
            "SELECT id, title, folder_path, deleted_at FROM meetings
             WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC",
        )
        .fetch_all(pool)
        .await
    }

    pub async fn delete_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<bool, SqlxError> {
        delete_meeting_checked(pool, meeting_id, false).await
    }

    /// Hard-delete a meeting only if it is still in Trash, checked and deleted in one
    /// transaction so a concurrent restore is never turned into a deletion.
    pub async fn delete_trashed_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<bool, SqlxError> {
        delete_meeting_checked(pool, meeting_id, true).await
    }

    pub async fn get_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<MeetingDetails>, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        // Get meeting details
        let meeting: Option<MeetingModel> =
            sqlx::query_as("SELECT id, title, created_at, updated_at, folder_path FROM meetings WHERE id = ?")
                .bind(meeting_id)
                .fetch_optional(&mut *transaction)
                .await?;

        if meeting.is_none() {
            transaction.rollback().await?;
            return Err(SqlxError::RowNotFound);
        }

        if let Some(meeting) = meeting {
            // Get all transcripts for this meeting
            let transcripts =
                sqlx::query_as::<_, Transcript>("SELECT * FROM transcripts WHERE meeting_id = ?")
                    .bind(meeting_id)
                    .fetch_all(&mut *transaction)
                    .await?;

            transaction.commit().await?;

            // Convert Transcript to MeetingTranscript
            let meeting_transcripts = transcripts
                .into_iter()
                .map(|t| MeetingTranscript {
                    id: t.id,
                    text: t.transcript,
                    timestamp: t.timestamp,
                    audio_start_time: t.audio_start_time,
                    audio_end_time: t.audio_end_time,
                    duration: t.duration,
                })
                .collect::<Vec<_>>();

            Ok(Some(MeetingDetails {
                id: meeting.id,
                title: meeting.title,
                created_at: meeting.created_at.0.to_rfc3339(),
                updated_at: meeting.updated_at.0.to_rfc3339(),
                transcripts: meeting_transcripts,
            }))
        } else {
            transaction.rollback().await?;
            Ok(None)
        }
    }

    /// Get meeting metadata without transcripts (for pagination)
    pub async fn get_meeting_metadata(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<MeetingModel>, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let meeting: Option<MeetingModel> =
            sqlx::query_as("SELECT id, title, created_at, updated_at, folder_path, folder_id, deleted_at FROM meetings WHERE id = ?")
                .bind(meeting_id)
                .fetch_optional(pool)
                .await?;

        Ok(meeting)
    }

    /// Get meeting transcripts with pagination support
    pub async fn get_meeting_transcripts_paginated(
        pool: &SqlitePool,
        meeting_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Transcript>, i64), SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        // Get total count of transcripts for this meeting
        let total: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM transcripts WHERE meeting_id = ?"
        )
        .bind(meeting_id)
        .fetch_one(pool)
        .await?;

        // Get paginated transcripts ordered by audio_start_time
        let transcripts = sqlx::query_as::<_, Transcript>(
            "SELECT * FROM transcripts
             WHERE meeting_id = ?
             ORDER BY audio_start_time ASC
             LIMIT ? OFFSET ?"
        )
        .bind(meeting_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok((transcripts, total.0))
    }

    pub async fn update_meeting_title(
        pool: &SqlitePool,
        meeting_id: &str,
        new_title: &str,
    ) -> Result<bool, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        let now = Utc::now().naive_utc();

        let rows_affected =
            sqlx::query("UPDATE meetings SET title = ?, updated_at = ? WHERE id = ?")
                .bind(new_title)
                .bind(now)
                .bind(meeting_id)
                .execute(&mut *transaction)
                .await?;
        if rows_affected.rows_affected() == 0 {
            transaction.rollback().await?;
            return Ok(false);
        }
        transaction.commit().await?;
        Ok(true)
    }

    pub async fn update_meeting_name(
        pool: &SqlitePool,
        meeting_id: &str,
        new_title: &str,
    ) -> Result<bool, SqlxError> {
        let mut transaction = pool.begin().await?;
        let now = Utc::now();

        // Update meetings table
        let meeting_update =
            sqlx::query("UPDATE meetings SET title = ?, updated_at = ? WHERE id = ?")
                .bind(new_title)
                .bind(now)
                .bind(meeting_id)
                .execute(&mut *transaction)
                .await?;

        if meeting_update.rows_affected() == 0 {
            transaction.rollback().await?;
            return Ok(false); // Meeting not found
        }

        // Update transcript_chunks table
        sqlx::query("UPDATE transcript_chunks SET meeting_name = ? WHERE meeting_id = ?")
            .bind(new_title)
            .bind(meeting_id)
            .execute(&mut *transaction)
            .await?;

        transaction.commit().await?;
        Ok(true)
    }
}

async fn delete_meeting_checked(
    pool: &SqlitePool,
    meeting_id: &str,
    require_trashed: bool,
) -> Result<bool, SqlxError> {
    if meeting_id.trim().is_empty() {
        return Err(SqlxError::Protocol(
            "meeting_id cannot be empty".to_string(),
        ));
    }

    let mut conn = pool.acquire().await?;
    let mut transaction = conn.begin().await?;

    if require_trashed {
        let trashed: Option<(i64,)> =
            sqlx::query_as("SELECT 1 FROM meetings WHERE id = ? AND deleted_at IS NOT NULL")
                .bind(meeting_id)
                .fetch_optional(&mut *transaction)
                .await?;
        if trashed.is_none() {
            transaction.rollback().await?;
            return Ok(false);
        }
    }

    match delete_meeting_with_transaction(&mut transaction, meeting_id).await {
        Ok(true) => {
            transaction.commit().await?;
            info!(
                "Successfully deleted meeting {} and all associated data",
                meeting_id
            );
            Ok(true)
        }
        Ok(false) => {
            transaction.rollback().await?;
            Ok(false)
        }
        Err(e) => {
            let _ = transaction.rollback().await;
            error!("Failed to delete meeting {}: {}", meeting_id, e);
            Err(e)
        }
    }
}

async fn delete_meeting_with_transaction(
    transaction: &mut SqliteConnection,
    meeting_id: &str,
) -> Result<bool, SqlxError> {
    // Check if meeting exists
    let meeting_exists: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(&mut *transaction)
        .await?;

    if meeting_exists.is_none() {
        error!("Meeting {} not found for deletion", meeting_id);
        return Ok(false);
    }

    // Delete from related tables in proper order
    // 1. Delete from transcript_chunks
    sqlx::query("DELETE FROM transcript_chunks WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    // 2. Delete from summary_processes
    sqlx::query("DELETE FROM summary_processes WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    // 3. Delete from transcripts
    sqlx::query("DELETE FROM transcripts WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    // 4. Finally, delete the meeting
    let result = sqlx::query("DELETE FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod org_tests {
    use super::*;
    use crate::database::repositories::folder::FolderRepository;
    use crate::database::repositories::org_error::OrgError;
    use crate::database::test_support::{insert_meeting, test_pool};

    #[tokio::test]
    async fn trashed_meetings_leave_the_list_and_come_back_on_restore() {
        let pool = test_pool().await;
        insert_meeting(&pool, "meeting-1", "A", None).await;
        insert_meeting(&pool, "meeting-2", "B", None).await;

        MeetingsRepository::trash(&pool, "meeting-1").await.unwrap();
        let ids: Vec<String> = MeetingsRepository::get_meetings(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, vec!["meeting-2"]);
        assert_eq!(MeetingsRepository::list_trash(&pool).await.unwrap()[0].id, "meeting-1");

        // Still fetchable by id so it can be viewed from Trash.
        let meta = MeetingsRepository::get_meeting_metadata(&pool, "meeting-1")
            .await
            .unwrap()
            .unwrap();
        assert!(meta.deleted_at.is_some());

        MeetingsRepository::restore(&pool, "meeting-1").await.unwrap();
        assert_eq!(MeetingsRepository::get_meetings(&pool).await.unwrap().len(), 2);
        assert!(MeetingsRepository::list_trash(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn trash_and_restore_report_missing_meetings() {
        let pool = test_pool().await;
        assert!(matches!(
            MeetingsRepository::trash(&pool, "meeting-x").await,
            Err(OrgError::NotFound(_))
        ));
        insert_meeting(&pool, "meeting-1", "A", None).await;
        assert!(matches!(
            MeetingsRepository::restore(&pool, "meeting-1").await,
            Err(OrgError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn restore_falls_back_to_unfiled_when_folder_was_deleted() {
        let pool = test_pool().await;
        let folder = FolderRepository::create(&pool, "Work").await.unwrap();
        insert_meeting(&pool, "meeting-1", "A", None).await;
        MeetingsRepository::set_folder(&pool, "meeting-1", Some(&folder.id)).await.unwrap();
        MeetingsRepository::trash(&pool, "meeting-1").await.unwrap();
        // Simulate a folder tombstoned without moving its meetings (e.g. arrived via sync).
        sqlx::query("UPDATE folders SET deleted_at = datetime('now') WHERE id = ?")
            .bind(&folder.id)
            .execute(&pool)
            .await
            .unwrap();

        MeetingsRepository::restore(&pool, "meeting-1").await.unwrap();
        let meeting = &MeetingsRepository::get_meetings(&pool).await.unwrap()[0];
        assert_eq!(meeting.folder_id, None);
    }

    #[tokio::test]
    async fn set_folder_validates_folder_and_meeting() {
        let pool = test_pool().await;
        insert_meeting(&pool, "meeting-1", "A", None).await;
        let folder = FolderRepository::create(&pool, "Work").await.unwrap();

        MeetingsRepository::set_folder(&pool, "meeting-1", Some(&folder.id)).await.unwrap();
        assert_eq!(
            MeetingsRepository::get_meetings(&pool).await.unwrap()[0].folder_id.as_deref(),
            Some(folder.id.as_str())
        );
        MeetingsRepository::set_folder(&pool, "meeting-1", None).await.unwrap();
        assert_eq!(MeetingsRepository::get_meetings(&pool).await.unwrap()[0].folder_id, None);

        assert!(matches!(
            MeetingsRepository::set_folder(&pool, "meeting-1", Some("folder-missing")).await,
            Err(OrgError::NotFound(_))
        ));
        assert!(matches!(
            MeetingsRepository::set_folder(&pool, "meeting-x", None).await,
            Err(OrgError::NotFound(_))
        ));
    }
}
