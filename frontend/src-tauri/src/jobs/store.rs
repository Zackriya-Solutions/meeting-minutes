//! Persistence for the background job queue (`jobs` table, migration 20260706000000).
//!
//! All timing uses SQLite's `datetime('now')` (UTC, 'YYYY-MM-DD HH:MM:SS') so the
//! queue's notion of "eligible now" matches what the DB stores. Backoff is expressed
//! as a SQLite datetime modifier (e.g. `+20 seconds`).

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct JobRow {
    pub id: i64,
    pub kind: String,
    pub meeting_id: Option<String>,
    pub payload: String,
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub run_after: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnqueueOutcome {
    pub id: i64,
    pub created: bool,
}

/// Insert a new queued job. Returns its id.
pub async fn enqueue(
    pool: &SqlitePool,
    kind: &str,
    meeting_id: Option<&str>,
    payload: &serde_json::Value,
) -> Result<i64, sqlx::Error> {
    let payload_str = payload.to_string();
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO jobs (kind, meeting_id, payload, status, updated_at) \
         VALUES (?, ?, ?, 'queued', datetime('now')) RETURNING id",
    )
    .bind(kind)
    .bind(meeting_id)
    .bind(payload_str)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Enqueue at most one active job for a `(kind, meeting_id)` pair. The
/// `INSERT ... WHERE NOT EXISTS` is a single SQLite write statement, so two
/// callers cannot both observe the queue as empty and insert duplicates.
/// Completed and permanently failed jobs do not block a fresh repair attempt.
pub async fn enqueue_unique(
    pool: &SqlitePool,
    kind: &str,
    meeting_id: Option<&str>,
    payload: &serde_json::Value,
) -> Result<EnqueueOutcome, sqlx::Error> {
    let payload_str = payload.to_string();
    if let Some(id) = try_insert_unique(pool, kind, meeting_id, &payload_str).await? {
        return Ok(EnqueueOutcome { id, created: true });
    }

    if let Some(id) = find_active_job(pool, kind, meeting_id).await? {
        return Ok(EnqueueOutcome { id, created: false });
    }

    // The blocking job may have completed between the INSERT and SELECT above.
    // Retry the atomic insert once; if another caller wins that retry, return the
    // active job it created instead of surfacing a benign RowNotFound race.
    if let Some(id) = try_insert_unique(pool, kind, meeting_id, &payload_str).await? {
        return Ok(EnqueueOutcome { id, created: true });
    }

    let id = find_active_job(pool, kind, meeting_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(EnqueueOutcome { id, created: false })
}

async fn try_insert_unique(
    pool: &SqlitePool,
    kind: &str,
    meeting_id: Option<&str>,
    payload: &str,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO jobs (kind, meeting_id, payload, status, updated_at) \
         SELECT ?, ?, ?, 'queued', datetime('now') \
         WHERE NOT EXISTS ( \
           SELECT 1 FROM jobs \
           WHERE kind = ? AND meeting_id IS ? AND status IN ('queued', 'running') \
         ) \
         RETURNING id",
    )
    .bind(kind)
    .bind(meeting_id)
    .bind(payload)
    .bind(kind)
    .bind(meeting_id)
    .fetch_optional(pool)
    .await
}

async fn find_active_job(
    pool: &SqlitePool,
    kind: &str,
    meeting_id: Option<&str>,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(
        "SELECT id FROM jobs \
         WHERE kind = ? AND meeting_id IS ? AND status IN ('queued', 'running') \
         ORDER BY id LIMIT 1",
    )
    .bind(kind)
    .bind(meeting_id)
    .fetch_optional(pool)
    .await
}

/// Startup recovery: any job left in `running` (app killed mid-flight) is returned to
/// `queued` so it is retried. Returns the number of jobs recovered.
pub async fn recover_running(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    let res = sqlx::query(
        "UPDATE jobs SET status='queued', updated_at=datetime('now') WHERE status='running'",
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// Fetch up to `limit` jobs eligible to run now (queued and past their backoff),
/// oldest first. Claiming is a separate, atomic step (`try_claim`).
pub async fn fetch_eligible(pool: &SqlitePool, limit: i64) -> Result<Vec<JobRow>, sqlx::Error> {
    sqlx::query_as::<_, JobRow>(
        "SELECT id, kind, meeting_id, payload, status, attempts, last_error, run_after \
         FROM jobs \
         WHERE status='queued' AND (run_after IS NULL OR run_after <= datetime('now')) \
         ORDER BY id ASC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Atomically claim a queued job by flipping it to `running`. Returns true iff this
/// caller won the claim (guards against double-processing).
pub async fn try_claim(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    let res = sqlx::query(
        "UPDATE jobs SET status='running', attempts=attempts+1, updated_at=datetime('now') \
         WHERE id=? AND status='queued'",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() == 1)
}

pub async fn mark_done(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE jobs SET status='done', last_error=NULL, updated_at=datetime('now') WHERE id=?",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record a failed attempt. If more attempts remain, requeue with a backoff delay;
/// otherwise mark permanently `failed`.
pub async fn mark_failed_or_retry(
    pool: &SqlitePool,
    id: i64,
    attempts: i64,
    max_attempts: i64,
    backoff_seconds: i64,
    error: &str,
) -> Result<(), sqlx::Error> {
    if attempts < max_attempts {
        let modifier = format!("+{backoff_seconds} seconds");
        sqlx::query(
            "UPDATE jobs SET status='queued', last_error=?, \
             run_after=datetime('now', ?), updated_at=datetime('now') WHERE id=?",
        )
        .bind(error)
        .bind(modifier)
        .bind(id)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            "UPDATE jobs SET status='failed', last_error=?, updated_at=datetime('now') WHERE id=?",
        )
        .bind(error)
        .bind(id)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Whether a meeting has any incomplete (queued/running) or failed jobs — used by the
/// UI to show a "processing incomplete" badge (PLAN.md §9 error convention).
pub async fn meeting_has_incomplete_jobs(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM jobs WHERE meeting_id=? AND status IN ('queued','running','failed')",
    )
    .bind(meeting_id)
    .fetch_one(pool)
    .await?;
    Ok(n > 0)
}
