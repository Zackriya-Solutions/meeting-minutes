//! The job runner: a single background task that claims eligible jobs, runs their
//! handlers with bounded concurrency, and applies retry/backoff (PLAN.md Phase 0 §3).

use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tokio::sync::{Notify, Semaphore};

#[cfg(test)]
use super::JobHandler;
use super::{store, JobContext, JobRegistry};

#[derive(Debug, Clone, Copy)]
pub struct RunnerConfig {
    /// Max jobs running at once.
    pub max_concurrent: usize,
    /// Total attempts before a job is marked permanently `failed`.
    pub max_attempts: i64,
    /// Base for exponential backoff: delay = base * 2^(attempts_made - 1) seconds.
    pub base_backoff_seconds: i64,
    /// Fallback poll cadence when no enqueue notification arrives.
    pub poll_interval: Duration,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 2,
            max_attempts: 3,
            base_backoff_seconds: 10,
            poll_interval: Duration::from_secs(3),
        }
    }
}

pub struct JobRunner {
    pool: SqlitePool,
    registry: Arc<JobRegistry>,
    config: RunnerConfig,
    notify: Arc<Notify>,
    semaphore: Arc<Semaphore>,
}

impl JobRunner {
    pub fn new(pool: SqlitePool, registry: JobRegistry, config: RunnerConfig) -> Self {
        Self {
            pool,
            registry: Arc::new(registry),
            semaphore: Arc::new(Semaphore::new(config.max_concurrent)),
            notify: Arc::new(Notify::new()),
            config,
        }
    }

    /// Notify handle so external enqueuers can wake the runner immediately.
    pub fn notify_handle(&self) -> Arc<Notify> {
        self.notify.clone()
    }

    /// Build a context handlers can use to enqueue follow-up jobs.
    fn context(&self) -> JobContext {
        JobContext {
            pool: self.pool.clone(),
            notify: self.notify.clone(),
        }
    }

    /// Spawn the runner loop on the tokio runtime. Consumes `self`.
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move { self.run_loop().await })
    }

    async fn run_loop(self) {
        match store::retire_legacy_startup_backfill_fanout(&self.pool).await {
            Ok(n) if n > 0 => {
                log::info!("job runner retired {n} legacy startup archive job(s)")
            }
            Ok(_) => {}
            Err(e) => log::error!("job runner legacy startup cleanup failed: {e}"),
        }

        // Startup recovery: requeue anything left `running` from a previous run.
        match store::recover_running(&self.pool).await {
            Ok(n) if n > 0 => log::info!("job runner recovered {n} interrupted job(s)"),
            Ok(_) => {}
            Err(e) => log::error!("job runner recovery failed: {e}"),
        }

        log::info!(
            "job runner started (max_concurrent={}, max_attempts={})",
            self.config.max_concurrent,
            self.config.max_attempts
        );

        loop {
            self.tick().await;

            // Wait for an enqueue notification or the poll interval, whichever first.
            tokio::select! {
                _ = self.notify.notified() => {}
                _ = tokio::time::sleep(self.config.poll_interval) => {}
            }
        }
    }

    /// Claim and dispatch as many eligible jobs as there are free permits.
    async fn tick(&self) {
        let available = self.semaphore.available_permits();
        if available == 0 {
            return;
        }

        let eligible = match store::fetch_eligible(&self.pool, available as i64).await {
            Ok(rows) => rows,
            Err(e) => {
                log::error!("job runner: fetch_eligible failed: {e}");
                return;
            }
        };

        for row in eligible {
            // Reserve a concurrency slot; stop if none are free right now.
            let permit = match self.semaphore.clone().try_acquire_owned() {
                Ok(p) => p,
                Err(_) => break,
            };

            // Atomically claim; if we lose the race (or it vanished), release + skip.
            match store::try_claim(&self.pool, row.id).await {
                Ok(true) => {}
                Ok(false) => continue,
                Err(e) => {
                    log::error!("job runner: claim failed for job {}: {e}", row.id);
                    continue;
                }
            }

            let pool = self.pool.clone();
            let registry = self.registry.clone();
            let ctx = self.context();
            let notify = self.notify.clone();
            let config = self.config;

            tokio::spawn(async move {
                let _permit = permit; // held until the job finishes
                run_one(&pool, &registry, &ctx, &config, row).await;
                // Wake the loop so freed capacity is refilled promptly.
                notify.notify_one();
            });
        }
    }
}

/// Test-only: run a single already-claimed job against an explicit handler and
/// record its outcome, using the same success/retry/backoff logic as [`run_one`].
#[cfg(test)]
pub(crate) async fn run_one_for_test(
    pool: &SqlitePool,
    handler: &impl JobHandler,
    ctx: &JobContext,
    config: &RunnerConfig,
    row: store::JobRow,
) {
    let attempts_made = row.attempts + 1;
    let payload: serde_json::Value =
        serde_json::from_str(&row.payload).unwrap_or_else(|_| serde_json::json!({}));
    match handler.run(ctx, row.meeting_id.as_deref(), &payload).await {
        Ok(()) => {
            store::mark_done(pool, row.id).await.unwrap();
        }
        Err(e) => {
            let err = e.to_string();
            let exp = (attempts_made - 1).clamp(0, 16) as u32;
            let backoff = config.base_backoff_seconds.saturating_mul(2i64.pow(exp));
            store::mark_failed_or_retry(
                pool,
                row.id,
                attempts_made,
                config.max_attempts,
                backoff,
                &err,
            )
            .await
            .unwrap();
        }
    }
}

/// Execute a single claimed job and record its outcome.
async fn run_one(
    pool: &SqlitePool,
    registry: &JobRegistry,
    ctx: &JobContext,
    config: &RunnerConfig,
    row: store::JobRow,
) {
    // `try_claim` already incremented attempts in the DB; row.attempts is pre-claim.
    let attempts_made = row.attempts + 1;
    let payload: serde_json::Value =
        serde_json::from_str(&row.payload).unwrap_or_else(|_| serde_json::json!({}));

    let Some(handler) = registry.get(&row.kind) else {
        log::error!(
            "job {} has unknown kind '{}'; marking failed",
            row.id,
            row.kind
        );
        // Force the permanent-failure branch (attempts == max).
        let _ = store::mark_failed_or_retry(
            pool,
            row.id,
            config.max_attempts,
            config.max_attempts,
            0,
            "no handler registered for this job kind",
        )
        .await;
        return;
    };

    log::debug!(
        "running job {} kind={} (attempt {})",
        row.id,
        row.kind,
        attempts_made
    );
    match handler.run(ctx, row.meeting_id.as_deref(), &payload).await {
        Ok(()) => {
            if let Err(e) = store::mark_done(pool, row.id).await {
                log::error!("failed to mark job {} done: {e}", row.id);
            }
        }
        Err(e) => {
            let err = e.to_string();
            // delay = base * 2^(attempts_made - 1)
            let exp = (attempts_made - 1).clamp(0, 16) as u32;
            let backoff = config.base_backoff_seconds.saturating_mul(2i64.pow(exp));
            log::warn!(
                "job {} kind={} failed (attempt {}/{}): {err}",
                row.id,
                row.kind,
                attempts_made,
                config.max_attempts
            );
            if let Err(e2) = store::mark_failed_or_retry(
                pool,
                row.id,
                attempts_made,
                config.max_attempts,
                backoff,
                &err,
            )
            .await
            {
                log::error!("failed to record failure for job {}: {e2}", row.id);
            }
        }
    }
}
