use crate::database::models::SummaryProcess;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::SqlitePool;
use tracing::{error, info as log_info};

pub struct SummaryProcessesRepository;

impl SummaryProcessesRepository {
    /// Retrieves the current summary process state for a given meeting ID.
    pub async fn get_summary_data(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<SummaryProcess>, sqlx::Error> {
        sqlx::query_as::<_, SummaryProcess>("SELECT * FROM summary_processes WHERE meeting_id = ?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await
    }

    pub async fn update_meeting_summary(
        pool: &SqlitePool,
        meeting_id: &str,
        summary: &Value,
    ) -> Result<bool, sqlx::Error> {
        let mut transaction = pool.begin().await?;

        let meeting_exists: bool = sqlx::query("SELECT 1 FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(&mut *transaction)
            .await?
            .is_some();

        if !meeting_exists {
            log_info!(
                "Attempted to save summary for a non-existent meeting_id: {}",
                meeting_id
            );
            transaction.rollback().await?;
            return Ok(false);
        }

        let result_json = serde_json::to_string(summary);
        if result_json.is_err() {
            error!("Can't convert the json to string for saving to Database");
            transaction.rollback().await?;
            return Ok(false);
        }
        let now = Utc::now();

        sqlx::query("UPDATE summary_processes SET result = ?, updated_at = ? WHERE meeting_id = ?")
            .bind(&result_json.unwrap())
            .bind(now)
            .bind(meeting_id)
            .execute(&mut *transaction)
            .await?;

        sqlx::query("UPDATE meetings SET updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(meeting_id)
            .execute(&mut *transaction)
            .await?;

        transaction.commit().await?;

        log_info!(
            "Successfully updated summary and timestamp for meeting_id: {}",
            meeting_id
        );
        Ok(true)
    }

    pub async fn get_summary_data_for_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<SummaryProcess>, sqlx::Error> {
        // A completed summary is durable user data. `transcript_chunks` is only generation
        // input/cache and may be absent after migration, cleanup, or an interrupted rewrite.
        // Joining it here made a valid persisted summary look like `idle` in the UI.
        Self::get_summary_data(pool, meeting_id).await
    }

    pub async fn create_or_reset_process(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<(), sqlx::Error> {
        log_info!(
            "Creating or resetting summary process for meeting_id: {}",
            meeting_id
        );
        let now = Utc::now();
        sqlx::query(
            r#"
            INSERT INTO summary_processes (meeting_id, status, created_at, updated_at, start_time, result, error)
            VALUES (?, 'PENDING', ?, ?, ?, NULL, NULL)
            ON CONFLICT(meeting_id) DO UPDATE SET
                status = 'PENDING',
                updated_at = excluded.updated_at,
                start_time = excluded.start_time,
                result_backup = COALESCE(result, result_backup),
                result_backup_timestamp = CASE
                    WHEN result IS NOT NULL THEN excluded.updated_at
                    ELSE result_backup_timestamp
                END,
                result = result,
                error = NULL
            "#
        )
        .bind(meeting_id)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await?;
        log_info!(
            "Backed up existing summary before regeneration for meeting_id: {}",
            meeting_id
        );
        Ok(())
    }

    /// Marks generations that were still running when the process exited.
    ///
    /// A generation runs inside a spawned task, so it cannot outlive the application. Any row
    /// left in PENDING/PROCESSING at startup therefore has no worker behind it, and nothing
    /// else ever moves it on: the UI keeps polling that status (a spinner that never stops)
    /// and `start_automatic_summary_for_meeting` skips the meeting as "already running".
    /// Clearing `metadata` drops the automatic-summary version marker so the startup backfill
    /// is allowed to retry the interrupted meeting exactly once per launch.
    pub async fn fail_interrupted_processes(
        pool: &SqlitePool,
        error: &str,
        started_before: DateTime<Utc>,
    ) -> Result<Vec<String>, sqlx::Error> {
        // `started_before` is the launch instant of the current process: a generation that
        // began after it is alive and must not be touched. A row with no start_time predates
        // that column and can only be a leftover.
        const RUNNING_AND_ABANDONED: &str = "lower(status) IN ('pending', 'processing') \
             AND (start_time IS NULL OR start_time < ?)";

        let meeting_ids: Vec<String> = sqlx::query_scalar(&format!(
            "SELECT meeting_id FROM summary_processes WHERE {RUNNING_AND_ABANDONED}"
        ))
        .bind(started_before)
        .fetch_all(pool)
        .await?;

        if meeting_ids.is_empty() {
            return Ok(meeting_ids);
        }

        let now = Utc::now();
        sqlx::query(&format!(
            r#"
            UPDATE summary_processes
            SET
                status = 'failed',
                error = ?,
                updated_at = ?,
                end_time = ?,
                result = COALESCE(result_backup, result),
                result_backup = NULL,
                result_backup_timestamp = NULL,
                metadata = NULL
            WHERE {RUNNING_AND_ABANDONED}
            "#
        ))
        .bind(error)
        .bind(now)
        .bind(now)
        .bind(started_before)
        .execute(pool)
        .await?;

        log_info!(
            "Recovered {} interrupted summary generation(s): {:?}",
            meeting_ids.len(),
            meeting_ids
        );
        Ok(meeting_ids)
    }

    pub async fn update_process_completed(
        pool: &SqlitePool,
        meeting_id: &str,
        result: Value, // Keep this as Value to handle both old and new formats if needed
        chunk_count: i64,
        processing_time: f64,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        let result_str = serde_json::to_string(&result)
            .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize result: {}", e)))?;

        sqlx::query(
            r#"
            UPDATE summary_processes
            SET status = 'completed', result = ?, updated_at = ?, end_time = ?, chunk_count = ?, processing_time = ?, error = NULL, result_backup = NULL, result_backup_timestamp = NULL
            WHERE meeting_id = ?
            "#
        )
        .bind(result_str)
        .bind(now)
        .bind(now)
        .bind(chunk_count)
        .bind(processing_time)
        .bind(meeting_id)
        .execute(pool)
        .await?;
        log_info!(
            "Summary completed and backup cleared for meeting_id: {}",
            meeting_id
        );
        Ok(())
    }

    pub async fn update_process_failed(
        pool: &SqlitePool,
        meeting_id: &str,
        error: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();

        // Restore from backup if it exists, otherwise keep current result
        sqlx::query(
            r#"
            UPDATE summary_processes
            SET
                status = 'failed',
                error = ?,
                updated_at = ?,
                end_time = ?,
                result = COALESCE(result_backup, result),
                result_backup = NULL,
                result_backup_timestamp = NULL
            WHERE meeting_id = ?
            "#,
        )
        .bind(error)
        .bind(now)
        .bind(now)
        .bind(meeting_id)
        .execute(pool)
        .await?;
        log_info!(
            "Summary generation failed and backup restored for meeting_id: {}",
            meeting_id
        );
        Ok(())
    }

    pub async fn update_process_cancelled(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();

        // Restore from backup if it exists, otherwise keep current result
        sqlx::query(
            r#"
            UPDATE summary_processes
            SET
                status = 'cancelled',
                updated_at = ?,
                end_time = ?,
                error = 'Generation was cancelled by user',
                result = COALESCE(result_backup, result),
                result_backup = NULL,
                result_backup_timestamp = NULL
            WHERE meeting_id = ?
            "#,
        )
        .bind(now)
        .bind(now)
        .bind(meeting_id)
        .execute(pool)
        .await?;
        log_info!(
            "Marked summary process as cancelled and restored backup for meeting_id: {}",
            meeting_id
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn persisted_summary_does_not_depend_on_transcript_chunk_cache() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE summary_processes (
                meeting_id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                error TEXT,
                result TEXT,
                start_time TEXT,
                end_time TEXT,
                chunk_count INTEGER NOT NULL DEFAULT 0,
                processing_time REAL NOT NULL DEFAULT 0,
                metadata TEXT,
                result_backup TEXT,
                result_backup_timestamp TEXT
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO summary_processes \
             (meeting_id, status, created_at, updated_at, result) \
             VALUES ('meeting-1', 'completed', '2026-07-18T10:00:00Z', \
                     '2026-07-18T10:01:00Z', '{\"markdown\":\"durable\"}')",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Deliberately do not create transcript_chunks: it is not part of summary ownership.
        let summary = SummaryProcessesRepository::get_summary_data_for_meeting(&pool, "meeting-1")
            .await
            .unwrap()
            .expect("summary remains visible");

        assert_eq!(summary.status, "completed");
        assert!(summary.result.unwrap().contains("durable"));
    }

    async fn schema_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE summary_processes (
                meeting_id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                error TEXT,
                result TEXT,
                start_time TEXT,
                end_time TEXT,
                chunk_count INTEGER NOT NULL DEFAULT 0,
                processing_time REAL NOT NULL DEFAULT 0,
                metadata TEXT,
                result_backup TEXT,
                result_backup_timestamp TEXT
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    async fn insert_process(pool: &SqlitePool, meeting_id: &str, status: &str, start_time: &str) {
        sqlx::query(
            "INSERT INTO summary_processes \
             (meeting_id, status, created_at, updated_at, start_time, metadata) \
             VALUES (?, ?, ?, ?, ?, '{\"automatic_summary_version\":\"automatic_summary_v2\"}')",
        )
        .bind(meeting_id)
        .bind(status)
        .bind(start_time)
        .bind(start_time)
        .bind(start_time)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn interrupted_generations_are_failed_and_left_retryable() {
        let pool = schema_pool().await;
        insert_process(&pool, "meeting-stale", "PENDING", "2026-08-05T17:34:09Z").await;

        let launched_at = "2026-08-06T09:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let recovered = SummaryProcessesRepository::fail_interrupted_processes(
            &pool,
            "interrupted",
            launched_at,
        )
        .await
        .unwrap();

        assert_eq!(recovered, vec!["meeting-stale".to_string()]);
        let row = SummaryProcessesRepository::get_summary_data(&pool, "meeting-stale")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.status, "failed");
        assert_eq!(row.error.as_deref(), Some("interrupted"));
        // The automatic-summary marker is gone, so the startup backfill may retry this meeting.
        assert!(row.metadata.is_none());
    }

    #[tokio::test]
    async fn recovery_restores_the_backup_of_an_interrupted_regeneration() {
        let pool = schema_pool().await;
        insert_process(&pool, "meeting-regen", "PENDING", "2026-08-05T17:34:09Z").await;
        sqlx::query(
            "UPDATE summary_processes SET result = NULL, result_backup = '{\"markdown\":\"previous\"}' \
             WHERE meeting_id = 'meeting-regen'",
        )
        .execute(&pool)
        .await
        .unwrap();

        let launched_at = "2026-08-06T09:00:00Z".parse::<DateTime<Utc>>().unwrap();
        SummaryProcessesRepository::fail_interrupted_processes(&pool, "interrupted", launched_at)
            .await
            .unwrap();

        let row = SummaryProcessesRepository::get_summary_data(&pool, "meeting-regen")
            .await
            .unwrap()
            .unwrap();
        assert!(row.result.unwrap().contains("previous"));
        assert!(row.result_backup.is_none());
    }

    #[tokio::test]
    async fn recovery_leaves_generations_started_in_this_session_alone() {
        let pool = schema_pool().await;
        insert_process(&pool, "meeting-live", "PENDING", "2026-08-06T09:00:30Z").await;

        let launched_at = "2026-08-06T09:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let recovered = SummaryProcessesRepository::fail_interrupted_processes(
            &pool,
            "interrupted",
            launched_at,
        )
        .await
        .unwrap();

        assert!(recovered.is_empty());
        let row = SummaryProcessesRepository::get_summary_data(&pool, "meeting-live")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.status, "PENDING");
    }
}
