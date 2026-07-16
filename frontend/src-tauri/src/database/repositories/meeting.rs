use crate::api::{MeetingDetails, MeetingTranscript};
use crate::database::models::{MeetingModel, Transcript};
use chrono::Utc;
use sqlx::{Connection, Error as SqlxError, SqliteConnection, SqlitePool};
use tracing::{error, info};

pub struct MeetingsRepository;

impl MeetingsRepository {
    pub async fn get_meetings(pool: &SqlitePool) -> Result<Vec<MeetingModel>, sqlx::Error> {
        let meetings =
            sqlx::query_as::<_, MeetingModel>("SELECT * FROM meetings ORDER BY created_at DESC")
                .fetch_all(pool)
                .await?;
        Ok(meetings)
    }

    pub async fn delete_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<bool, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        match delete_meeting_with_transaction(&mut transaction, meeting_id).await {
            Ok(success) => {
                if success {
                    transaction.commit().await?;
                    info!(
                        "Successfully deleted meeting {} and all associated data",
                        meeting_id
                    );
                    Ok(true)
                } else {
                    transaction.rollback().await?;
                    Ok(false)
                }
            }
            Err(e) => {
                let _ = transaction.rollback().await;
                error!("Failed to delete meeting {}: {}", meeting_id, e);
                Err(e)
            }
        }
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
        let meeting: Option<MeetingModel> = sqlx::query_as(
            "SELECT id, title, created_at, updated_at, folder_path FROM meetings WHERE id = ?",
        )
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
                    speaker: t.speaker,
                    speaker_id: t.speaker_id,
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

        let meeting: Option<MeetingModel> = sqlx::query_as(
            "SELECT id, title, created_at, updated_at, folder_path FROM meetings WHERE id = ?",
        )
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?;

        Ok(meeting)
    }

    /// Per-meeting diarization preferences (set from the in-recording control pill).
    /// `enabled = None` means "follow the default" (enabled); `expected_speakers = None`
    /// means automatic estimation. Returns `None` when the meeting does not exist.
    pub async fn get_diarization_prefs(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<(Option<bool>, Option<i64>)>, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let row: Option<(Option<i64>, Option<i64>)> = sqlx::query_as(
            "SELECT diarization_enabled, expected_speakers FROM meetings WHERE id = ?",
        )
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|(enabled, expected)| (enabled.map(|v| v != 0), expected)))
    }

    /// Store per-meeting diarization preferences. Returns false when the meeting
    /// does not exist. NULLs reset a preference back to its default.
    pub async fn set_diarization_prefs(
        pool: &SqlitePool,
        meeting_id: &str,
        enabled: Option<bool>,
        expected_speakers: Option<i64>,
    ) -> Result<bool, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let rows_affected = sqlx::query(
            "UPDATE meetings SET diarization_enabled = ?, expected_speakers = ? WHERE id = ?",
        )
        .bind(enabled.map(|v| v as i64))
        .bind(expected_speakers)
        .bind(meeting_id)
        .execute(pool)
        .await?;

        Ok(rows_affected.rows_affected() > 0)
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
        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM transcripts WHERE meeting_id = ?")
            .bind(meeting_id)
            .fetch_one(pool)
            .await?;

        // Get paginated transcripts ordered by audio_start_time
        let transcripts = sqlx::query_as::<_, Transcript>(
            "SELECT * FROM transcripts
             WHERE meeting_id = ?
             ORDER BY audio_start_time ASC
             LIMIT ? OFFSET ?",
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

    // SQLite foreign-key enforcement is not guaranteed on every existing app
    // connection, so meeting-scoped data is removed explicitly.
    sqlx::query("DELETE FROM action_items WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    sqlx::query("DELETE FROM standup_records WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    sqlx::query("DELETE FROM standup_private_notes WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    sqlx::query("DELETE FROM meeting_collections WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    sqlx::query("DELETE FROM pending_merges WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    sqlx::query("DELETE FROM entity_mentions WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    sqlx::query("DELETE FROM speaker_name_candidates WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    sqlx::query("DELETE FROM rejected_speaker_name_candidate_instances WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    // Vector rows are not foreign-key linked to chunks. Remove them before the
    // source chunks so deleting a meeting cannot leave searchable transcript
    // content behind when foreign-key enforcement is disabled.
    let embeddings_table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master \
         WHERE type = 'table' AND name = 'chunk_embeddings')",
    )
    .fetch_one(&mut *transaction)
    .await?;
    if embeddings_table_exists {
        sqlx::query(
            "DELETE FROM chunk_embeddings \
             WHERE chunk_id IN (SELECT id FROM chunks WHERE meeting_id = ?)",
        )
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;
    }

    sqlx::query("DELETE FROM chunks WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    sqlx::query("DELETE FROM jobs WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    // Delete from the older related tables in proper order.
    sqlx::query("DELETE FROM transcript_chunks WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    sqlx::query("DELETE FROM summary_processes WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    sqlx::query("DELETE FROM transcripts WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    sqlx::query("DELETE FROM app_settings_kv WHERE key = ?")
        .bind(crate::summary::content_window::preference_key(meeting_id))
        .execute(&mut *transaction)
        .await?;

    // Finally, delete the meeting itself.
    let result = sqlx::query("DELETE FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory pool with the meetings columns the diarization-prefs SQL touches
    /// (mirrors the real schema's relevant subset incl. migration 20260714000000).
    async fn prefs_test_pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open in-memory db");
        sqlx::query(
            "CREATE TABLE meetings (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                diarization_enabled INTEGER,
                expected_speakers INTEGER
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO meetings (id, title) VALUES ('m1', 'Standup')")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn diarization_prefs_default_to_none_and_round_trip() {
        let pool = prefs_test_pool().await;

        // Untouched meeting: both prefs unset.
        let prefs = MeetingsRepository::get_diarization_prefs(&pool, "m1")
            .await
            .unwrap();
        assert_eq!(prefs, Some((None, None)));

        // Pill choices: speaker ID off, 3 expected speakers.
        let updated = MeetingsRepository::set_diarization_prefs(&pool, "m1", Some(false), Some(3))
            .await
            .unwrap();
        assert!(updated);
        let prefs = MeetingsRepository::get_diarization_prefs(&pool, "m1")
            .await
            .unwrap();
        assert_eq!(prefs, Some((Some(false), Some(3))));

        // Nulls reset back to defaults.
        MeetingsRepository::set_diarization_prefs(&pool, "m1", None, None)
            .await
            .unwrap();
        let prefs = MeetingsRepository::get_diarization_prefs(&pool, "m1")
            .await
            .unwrap();
        assert_eq!(prefs, Some((None, None)));
    }

    #[tokio::test]
    async fn diarization_prefs_missing_meeting_and_empty_id() {
        let pool = prefs_test_pool().await;
        assert_eq!(
            MeetingsRepository::get_diarization_prefs(&pool, "nope")
                .await
                .unwrap(),
            None
        );
        assert!(
            !MeetingsRepository::set_diarization_prefs(&pool, "nope", Some(true), None)
                .await
                .unwrap()
        );
        assert!(MeetingsRepository::get_diarization_prefs(&pool, "  ")
            .await
            .is_err());
        assert!(
            MeetingsRepository::set_diarization_prefs(&pool, "", None, None)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn deleting_meeting_removes_standup_and_private_workflow_data() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for ddl in [
            "CREATE TABLE meetings(id TEXT PRIMARY KEY, title TEXT)",
            "CREATE TABLE action_items(id INTEGER PRIMARY KEY, meeting_id TEXT)",
            "CREATE TABLE standup_records(id INTEGER PRIMARY KEY, meeting_id TEXT)",
            "CREATE TABLE standup_private_notes(id INTEGER PRIMARY KEY, meeting_id TEXT, text TEXT)",
            "CREATE TABLE meeting_collections(meeting_id TEXT, collection_id INTEGER)",
            "CREATE TABLE pending_merges(id INTEGER PRIMARY KEY, meeting_id TEXT)",
            "CREATE TABLE entity_mentions(id INTEGER PRIMARY KEY, meeting_id TEXT)",
            "CREATE TABLE speaker_name_candidates(id INTEGER PRIMARY KEY, meeting_id TEXT)",
            "CREATE TABLE rejected_speaker_name_candidate_instances(id INTEGER PRIMARY KEY, meeting_id TEXT)",
            "CREATE TABLE chunks(id INTEGER PRIMARY KEY, meeting_id TEXT, text TEXT)",
            "CREATE TABLE chunk_embeddings(chunk_id INTEGER PRIMARY KEY)",
            "CREATE TABLE jobs(id INTEGER PRIMARY KEY, meeting_id TEXT)",
            "CREATE TABLE app_settings_kv(key TEXT PRIMARY KEY, value TEXT)",
            "CREATE TABLE transcript_chunks(id INTEGER PRIMARY KEY, meeting_id TEXT)",
            "CREATE TABLE summary_processes(id INTEGER PRIMARY KEY, meeting_id TEXT)",
            "CREATE TABLE transcripts(id INTEGER PRIMARY KEY, meeting_id TEXT)",
        ] {
            sqlx::query(ddl).execute(&pool).await.unwrap();
        }
        sqlx::query("INSERT INTO meetings VALUES('m1', 'Standup')")
            .execute(&pool)
            .await
            .unwrap();
        for statement in [
            "INSERT INTO action_items(meeting_id) VALUES('m1')",
            "INSERT INTO standup_records(meeting_id) VALUES('m1')",
            "INSERT INTO standup_private_notes(meeting_id, text) VALUES('m1', 'secret')",
            "INSERT INTO meeting_collections VALUES('m1', 1)",
            "INSERT INTO pending_merges(meeting_id) VALUES('m1')",
            "INSERT INTO entity_mentions(meeting_id) VALUES('m1')",
            "INSERT INTO speaker_name_candidates(meeting_id) VALUES('m1')",
            "INSERT INTO rejected_speaker_name_candidate_instances(meeting_id) VALUES('m1')",
            "INSERT INTO chunks(id, meeting_id, text) VALUES(42, 'm1', 'private transcript')",
            "INSERT INTO chunk_embeddings(chunk_id) VALUES(42)",
            "INSERT INTO jobs(meeting_id) VALUES('m1')",
            "INSERT INTO app_settings_kv(key, value) VALUES('summary.content_window.m1', 'primary:0:1000:2000:1')",
            "INSERT INTO transcript_chunks(meeting_id) VALUES('m1')",
            "INSERT INTO summary_processes(meeting_id) VALUES('m1')",
            "INSERT INTO transcripts(meeting_id) VALUES('m1')",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }

        assert!(MeetingsRepository::delete_meeting(&pool, "m1")
            .await
            .unwrap());
        for table in [
            "meetings",
            "action_items",
            "standup_records",
            "standup_private_notes",
            "meeting_collections",
            "pending_merges",
            "entity_mentions",
            "speaker_name_candidates",
            "rejected_speaker_name_candidate_instances",
            "chunks",
            "chunk_embeddings",
            "jobs",
            "app_settings_kv",
            "transcript_chunks",
            "summary_processes",
            "transcripts",
        ] {
            let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(count, 0, "{table} still contains meeting-scoped data");
        }
    }

}
