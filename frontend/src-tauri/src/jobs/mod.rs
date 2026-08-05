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

const AUTOMATIC_DIARIZATION_BACKFILL_SOURCE: &str = "automatic_diarization_backfill_v1";
const AUTOMATIC_TRANSCRIPT_REPAIR_SOURCE: &str = "automatic_transcript_repair_v1";

/// Job kind identifiers. Kept as constants so enqueue sites and handlers agree.
pub mod kind {
    pub const CHUNK_EMBED: &str = "chunk_embed";
    pub const EMBEDDING_REPAIR: &str = "embedding_repair";
    pub const DIARIZE: &str = "diarize";
    pub const NAME_SPEAKERS: &str = "name_speakers";
    pub const EXTRACT: &str = "extract";
    pub const BACKFILL: &str = "backfill";
    pub const AUDIO_IDENTITY_BACKFILL: &str = "audio_identity_backfill";
    pub const REFINE_MISSING_TRANSCRIPT: &str = "refine_missing_transcript";
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
            .register(Arc::new(handlers::NameSpeakersHandler))
            .register(Arc::new(handlers::ExtractHandler))
            .register(Arc::new(handlers::BackfillHandler))
            .register(Arc::new(handlers::AudioIdentityBackfillHandler))
            .register(Arc::new(handlers::RefineMissingTranscriptHandler));
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

/// Queue the automatic speaker-naming pass for a meeting whose voices have just been
/// separated. Kept out of the diarization run itself so a slow or unavailable model never
/// delays the speaker labels the user is waiting for, and so the naming attempt retries on
/// its own schedule.
pub async fn enqueue_speaker_naming(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<i64, sqlx::Error> {
    Ok(store::enqueue_unique(
        pool,
        kind::NAME_SPEAKERS,
        Some(meeting_id),
        &serde_json::json!({ "source": "post_diarization" }),
    )
    .await?
    .id)
}

/// Queue one repair pass for archived meetings that have a transcript and recording but
/// have never received any diarized speaker attribution. The versioned source marker keeps
/// a permanently unsupported/corrupt recording from being uploaded again on every launch.
/// New meetings normally run diarization immediately through post-meeting refinement; this
/// sweep is the resilient catch-up path for interrupted runs and older installations.
pub async fn enqueue_missing_diarization(pool: &SqlitePool) -> Result<usize, sqlx::Error> {
    let meeting_ids: Vec<String> = sqlx::query_scalar(
        "SELECT m.id FROM meetings m \
         WHERE m.folder_path IS NOT NULL AND length(trim(m.folder_path)) > 0 \
           AND COALESCE(m.diarization_enabled, 1) <> 0 \
           AND EXISTS (SELECT 1 FROM transcripts t WHERE t.meeting_id = m.id) \
           AND NOT EXISTS ( \
             SELECT 1 FROM transcripts t \
             WHERE t.meeting_id = m.id AND t.speaker_id IS NOT NULL \
           ) \
           AND NOT EXISTS ( \
             SELECT 1 FROM jobs j \
             WHERE j.kind = 'diarize' AND j.meeting_id = m.id \
               AND json_extract(j.payload, '$.source') = ? \
           ) \
         ORDER BY COALESCE(m.occurred_at, m.created_at), m.id",
    )
    .bind(AUTOMATIC_DIARIZATION_BACKFILL_SOURCE)
    .fetch_all(pool)
    .await?;

    let mut created = 0usize;
    for meeting_id in meeting_ids {
        if store::enqueue_unique(
            pool,
            kind::DIARIZE,
            Some(&meeting_id),
            &serde_json::json!({
                "run_analysis": true,
                "source": AUTOMATIC_DIARIZATION_BACKFILL_SOURCE,
            }),
        )
        .await?
        .created
        {
            created += 1;
        }
    }
    Ok(created)
}

/// Queue a one-time recovery pass for completed recordings that contain at least two
/// minutes of saved audio but no transcript rows. Diarization cannot label an empty
/// transcript, so the handler runs the same post-meeting refinement used by newly saved
/// meetings: transcribe from the recording first, then attribute speakers.
pub async fn enqueue_missing_transcript_refinement(
    pool: &SqlitePool,
) -> Result<usize, sqlx::Error> {
    let candidates: Vec<(String, String)> = sqlx::query_as(
        "SELECT m.id, m.folder_path FROM meetings m \
         WHERE m.folder_path IS NOT NULL AND length(trim(m.folder_path)) > 0 \
           AND COALESCE(m.diarization_enabled, 1) <> 0 \
           AND NOT EXISTS (SELECT 1 FROM transcripts t WHERE t.meeting_id = m.id) \
           AND NOT EXISTS ( \
             SELECT 1 FROM jobs j \
             WHERE j.kind = 'refine_missing_transcript' AND j.meeting_id = m.id \
               AND json_extract(j.payload, '$.source') = ? \
           ) \
         ORDER BY COALESCE(m.occurred_at, m.created_at), m.id",
    )
    .bind(AUTOMATIC_TRANSCRIPT_REPAIR_SOURCE)
    .fetch_all(pool)
    .await?;

    let mut created = 0usize;
    for (meeting_id, folder_path) in candidates {
        let metadata_path = std::path::Path::new(&folder_path).join("metadata.json");
        let metadata = match std::fs::read_to_string(metadata_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        {
            Some(metadata) => metadata,
            None => continue,
        };
        let is_completed =
            metadata.get("status").and_then(serde_json::Value::as_str) == Some("completed");
        let duration_seconds = metadata
            .get("duration_seconds")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or_default();
        if !is_completed || duration_seconds < 120.0 {
            continue;
        }

        if store::enqueue_unique(
            pool,
            kind::REFINE_MISSING_TRANSCRIPT,
            Some(&meeting_id),
            &serde_json::json!({
                "source": AUTOMATIC_TRANSCRIPT_REPAIR_SOURCE,
                "folder_path": folder_path,
            }),
        )
        .await?
        .created
        {
            created += 1;
        }
    }
    Ok(created)
}
