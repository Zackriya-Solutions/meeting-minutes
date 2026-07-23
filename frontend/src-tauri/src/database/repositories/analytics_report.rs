//! Persistence for Deep Analytics reports (`analytics_reports` table, migration
//! `20260723190000_analytics_reports.sql`).
//!
//! One row per report run. The pipeline moves a row through
//! `queued` -> `running` -> (`completed` | `failed` | `cancelled`) and records the
//! rendered HTML path plus a JSON snapshot of every stage's artifacts. The report
//! UI reads the LATEST row per meeting via [`AnalyticsReportsRepository::latest_for_meeting`].

use crate::database::models::AnalyticsReportMeta;
use sqlx::{Error as SqlxError, SqlitePool};

/// Total pipeline stages, mirrored into the row so the UI can render a progress bar
/// without hard-coding the count. Kept in sync with `report::pipeline::STAGE_META`.
pub const TOTAL_STAGES: i64 = 12;

pub struct AnalyticsReportsRepository;

impl AnalyticsReportsRepository {
    /// Insert a fresh `queued` report row.
    pub async fn insert(
        pool: &SqlitePool,
        id: &str,
        meeting_id: &str,
        model: &str,
    ) -> Result<(), SqlxError> {
        sqlx::query(
            "INSERT INTO analytics_reports \
             (id, meeting_id, status, stage_index, total_stages, model) \
             VALUES (?, ?, 'queued', 0, ?, ?)",
        )
        .bind(id)
        .bind(meeting_id)
        .bind(TOTAL_STAGES)
        .bind(model)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// The id of an in-flight (`queued`/`running`) report for this meeting, if any.
    /// Used to de-duplicate concurrent generate requests.
    pub async fn active_report_id_for_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<String>, SqlxError> {
        sqlx::query_scalar::<_, String>(
            "SELECT id FROM analytics_reports \
             WHERE meeting_id = ? AND status IN ('queued', 'running', 'waiting_input') \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
    }

    /// The latest report row for a meeting (any status), or `None` if never generated.
    pub async fn latest_for_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<AnalyticsReportMeta>, SqlxError> {
        sqlx::query_as::<_, AnalyticsReportMeta>(
            "SELECT id, meeting_id, status, stage, stage_index, total_stages, \
                    html_path, error, created_at, completed_at, questions \
             FROM analytics_reports WHERE meeting_id = ? \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
    }

    /// Record the start of a stage: status -> `running`, with the stage label + index.
    pub async fn update_stage(
        pool: &SqlitePool,
        id: &str,
        stage: &str,
        stage_index: i64,
    ) -> Result<(), SqlxError> {
        sqlx::query(
            "UPDATE analytics_reports SET status = 'running', stage = ?, stage_index = ? \
             WHERE id = ?",
        )
        .bind(stage)
        .bind(stage_index)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// The clarify stage produced questions: park the run in `waiting_input` and store
    /// the questions JSON so the frontend can render (and later restore) the ask screen.
    pub async fn set_questions_waiting(
        pool: &SqlitePool,
        id: &str,
        questions_json: &str,
    ) -> Result<(), SqlxError> {
        sqlx::query(
            "UPDATE analytics_reports SET status = 'waiting_input', questions = ? WHERE id = ?",
        )
        .bind(questions_json)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Answers arrived (or the wait timed out): resume `running` and persist answers JSON.
    pub async fn set_answers_running(
        pool: &SqlitePool,
        id: &str,
        answers_json: &str,
    ) -> Result<(), SqlxError> {
        sqlx::query("UPDATE analytics_reports SET status = 'running', answers = ? WHERE id = ?")
            .bind(answers_json)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Mark the report completed with the rendered HTML path and artifacts JSON.
    pub async fn mark_completed(
        pool: &SqlitePool,
        id: &str,
        html_path: &str,
        artifacts_json: &str,
    ) -> Result<(), SqlxError> {
        sqlx::query(
            "UPDATE analytics_reports \
             SET status = 'completed', html_path = ?, artifacts = ?, error = NULL, \
                 completed_at = datetime('now') \
             WHERE id = ?",
        )
        .bind(html_path)
        .bind(artifacts_json)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Mark the report failed with an error message.
    pub async fn mark_failed(pool: &SqlitePool, id: &str, error: &str) -> Result<(), SqlxError> {
        sqlx::query(
            "UPDATE analytics_reports \
             SET status = 'failed', error = ?, completed_at = datetime('now') WHERE id = ?",
        )
        .bind(error)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Mark the report cancelled (idempotent; only affects still-active rows).
    pub async fn mark_cancelled(pool: &SqlitePool, id: &str) -> Result<(), SqlxError> {
        sqlx::query(
            "UPDATE analytics_reports \
             SET status = 'cancelled', completed_at = datetime('now') \
             WHERE id = ? AND status IN ('queued', 'running', 'waiting_input')",
        )
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }
}
