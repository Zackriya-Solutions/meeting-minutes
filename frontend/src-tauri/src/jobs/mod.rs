//! Background job queue for post-meeting processing (PLAN.md Phase 0 §3).
//!
//! Meeting finalization enqueues a chain of jobs and returns immediately — the UI is
//! never blocked. Jobs are persisted in the `jobs` table so they survive restarts:
//! anything left `running` when the app dies is requeued on next launch.
//!
//! Job kinds are registered as [`JobHandler`]s. Phase 0 ships placeholder handlers for
//! the four kinds (`chunk_embed`, `diarize`, `extract`, `backfill`); later phases
//! replace them with real logic without touching the runner.
//!
//! Chain ordering already incorporates the Phase 2 degradation rule: `chunk_embed`
//! runs first (search must work even if diarization fails), then `diarize` and
//! `extract` are enqueued in parallel on success. A diarize failure therefore cannot
//! block search or extraction.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::SqlitePool;
use tokio::sync::Notify;

pub mod handlers;
pub mod runner;
pub mod store;

#[cfg(test)]
mod tests;

pub use runner::{JobRunner, RunnerConfig};
pub use store::{meeting_has_incomplete_jobs, JobRow};

/// Job kind identifiers. Kept as constants so enqueue sites and handlers agree.
pub mod kind {
    pub const CHUNK_EMBED: &str = "chunk_embed";
    pub const EMBEDDING_REPAIR: &str = "embedding_repair";
    pub const DIARIZE: &str = "diarize";
    pub const EXTRACT: &str = "extract";
    pub const BACKFILL: &str = "backfill";
    pub const AUDIO_IDENTITY_BACKFILL: &str = "audio_identity_backfill";
}

/// Context handed to a handler when its job runs. Handlers use it to reach the DB and
/// to enqueue follow-up jobs (which also wakes the runner).
#[derive(Clone)]
pub struct JobContext {
    pub pool: SqlitePool,
    notify: Arc<Notify>,
}

impl JobContext {
    /// Enqueue a follow-up job and wake the runner so it is picked up promptly.
    pub async fn enqueue(
        &self,
        kind: &str,
        meeting_id: Option<&str>,
        payload: &serde_json::Value,
    ) -> Result<i64, sqlx::Error> {
        let id = store::enqueue(&self.pool, kind, meeting_id, payload).await?;
        self.notify.notify_one();
        Ok(id)
    }

    /// Enqueue only when the same job is not already queued or running.
    pub async fn enqueue_unique(
        &self,
        kind: &str,
        meeting_id: Option<&str>,
        payload: &serde_json::Value,
    ) -> Result<store::EnqueueOutcome, sqlx::Error> {
        let outcome = store::enqueue_unique(&self.pool, kind, meeting_id, payload).await?;
        if outcome.created {
            self.notify.notify_one();
        }
        Ok(outcome)
    }
}

/// A unit of background work. `kind()` must return one of the `kind::*` identifiers.
#[async_trait]
pub trait JobHandler: Send + Sync {
    fn kind(&self) -> &'static str;
    async fn run(
        &self,
        ctx: &JobContext,
        meeting_id: Option<&str>,
        payload: &serde_json::Value,
    ) -> anyhow::Result<()>;
}

/// Maps a job kind to its handler.
#[derive(Default)]
pub struct JobRegistry {
    handlers: HashMap<&'static str, Arc<dyn JobHandler>>,
}

impl JobRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, handler: Arc<dyn JobHandler>) -> &mut Self {
        self.handlers.insert(handler.kind(), handler);
        self
    }

    pub fn get(&self, kind: &str) -> Option<Arc<dyn JobHandler>> {
        self.handlers.get(kind).cloned()
    }

    /// Registry with the default Phase 0 placeholder handlers registered.
    pub fn with_defaults() -> Self {
        let mut r = Self::new();
        r.register(Arc::new(handlers::ChunkEmbedHandler))
            .register(Arc::new(handlers::EmbeddingRepairHandler))
            .register(Arc::new(handlers::DiarizeHandler))
            .register(Arc::new(handlers::ExtractHandler))
            .register(Arc::new(handlers::BackfillHandler))
            .register(Arc::new(handlers::AudioIdentityBackfillHandler));
        r
    }
}

/// Enqueue the full post-meeting pipeline for a finalized meeting. Returns the id of
/// the entry-point (`chunk_embed`) job. Non-blocking: does not wait for processing.
pub async fn enqueue_post_meeting_pipeline(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<i64, sqlx::Error> {
    Ok(store::enqueue_unique(
        pool,
        kind::CHUNK_EMBED,
        Some(meeting_id),
        &serde_json::json!({ "run_analysis": true, "source": "post_meeting" }),
    )
    .await?
    .id)
}
