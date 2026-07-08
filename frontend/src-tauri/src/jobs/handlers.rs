//! Phase 0 placeholder job handlers.
//!
//! These make the pipeline wiring real and testable end-to-end while the actual
//! processing is implemented in later phases:
//!   * `chunk_embed` → Phase 1 (chunker + embedder)
//!   * `diarize`     → Phase 2 (diarization + speaker profiles)
//!   * `extract`     → Phase 3 (entities + action items)
//!   * `backfill`    → Phase 5 (archive backfill)
//!
//! The `chunk_embed` handler enqueues `diarize` and `extract` on success, encoding
//! the Phase 2 degradation rule (search-critical work first; a diarize failure never
//! blocks extraction).

use async_trait::async_trait;

use super::{kind, JobContext, JobHandler};
use crate::pipeline::chunker::{approx_token_count, chunk_segments, ChunkConfig, Segment};

pub struct ChunkEmbedHandler;

#[async_trait]
impl JobHandler for ChunkEmbedHandler {
    fn kind(&self) -> &'static str {
        kind::CHUNK_EMBED
    }

    async fn run(
        &self,
        ctx: &JobContext,
        meeting_id: Option<&str>,
        _payload: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let meeting_id = meeting_id.ok_or_else(|| anyhow::anyhow!("chunk_embed requires a meeting_id"))?;
        let pool = &ctx.pool;

        // Load segments (ordered). Timing is seconds (REAL) -> ms; NULLs degrade to 0.
        let rows: Vec<(String, String, Option<f64>, Option<f64>)> = sqlx::query_as(
            "SELECT id, transcript, audio_start_time, audio_end_time FROM transcripts \
             WHERE meeting_id = ? ORDER BY COALESCE(audio_start_time, 0.0), rowid",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await?;

        let segments: Vec<Segment> = rows
            .into_iter()
            .map(|(id, text, start, end)| Segment {
                id,
                text,
                start_ms: (start.unwrap_or(0.0) * 1000.0) as i64,
                end_ms: (end.unwrap_or(0.0) * 1000.0) as i64,
            })
            .collect();

        if segments.is_empty() {
            log::info!("[chunk_embed] meeting {meeting_id} has no segments; nothing to chunk");
        } else {
            let chunks = chunk_segments(&segments, &ChunkConfig::default(), approx_token_count);

            // Idempotent: replace any existing chunks for this meeting (deterministic
            // chunker makes this safe for backfill re-runs). Remove their embeddings too
            // (chunk_embeddings is not FK-linked to chunks).
            let old_ids: Vec<i64> =
                sqlx::query_scalar("SELECT id FROM chunks WHERE meeting_id = ?")
                    .bind(meeting_id)
                    .fetch_all(pool)
                    .await?;
            for id in old_ids {
                let _ = sqlx::query("DELETE FROM chunk_embeddings WHERE chunk_id = ?")
                    .bind(id)
                    .execute(pool)
                    .await; // best-effort; table may not exist without sqlite-vec
            }
            sqlx::query("DELETE FROM chunks WHERE meeting_id = ?")
                .bind(meeting_id)
                .execute(pool)
                .await?;

            // Insert chunks. The `chunks_fts_ai` trigger indexes them for BM25, so the
            // FTS branch of hybrid search works immediately — before any embeddings.
            let mut chunk_ids = Vec::with_capacity(chunks.len());
            for c in &chunks {
                let id: i64 = sqlx::query_scalar(
                    "INSERT INTO chunks \
                     (meeting_id, first_segment_id, last_segment_id, start_ms, end_ms, text, token_count) \
                     VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
                )
                .bind(meeting_id)
                .bind(&c.first_segment_id)
                .bind(&c.last_segment_id)
                .bind(c.start_ms)
                .bind(c.end_ms)
                .bind(&c.text)
                .bind(c.token_count as i64)
                .fetch_one(pool)
                .await?;
                chunk_ids.push(id);
            }
            log::info!("[chunk_embed] meeting {meeting_id}: created {} chunk(s)", chunk_ids.len());

            // Embedding step: embed each chunk and write to the vec0 table. If no model is
            // loaded (or sqlite-vec is unavailable), chunks stay embedding_status='pending'
            // and search runs on the FTS branch only — never fatal.
            if crate::pipeline::embedder::is_loaded() {
                let texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();
                match crate::pipeline::embedder::embed_passages(texts).await {
                    Some(Ok(vectors)) if vectors.len() == chunk_ids.len() => {
                        let mut embedded = 0usize;
                        for (id, vec) in chunk_ids.iter().zip(vectors) {
                            match crate::vector::upsert_embedding(pool, *id, &vec).await {
                                Ok(()) => {
                                    let _ = sqlx::query(
                                        "UPDATE chunks SET embedding_status='done' WHERE id=?",
                                    )
                                    .bind(*id)
                                    .execute(pool)
                                    .await;
                                    embedded += 1;
                                }
                                Err(e) => log::warn!("[chunk_embed] upsert failed for chunk {id}: {e}"),
                            }
                        }
                        log::info!("[chunk_embed] meeting {meeting_id}: embedded {embedded} chunk(s)");
                    }
                    Some(Ok(_)) => log::warn!("[chunk_embed] embedding count mismatch; skipping"),
                    Some(Err(e)) => {
                        log::warn!("[chunk_embed] embedding failed: {e}");
                        let _ = sqlx::query("UPDATE chunks SET embedding_status='failed' WHERE meeting_id=?")
                            .bind(meeting_id)
                            .execute(pool)
                            .await;
                    }
                    None => {}
                }
            } else {
                log::info!(
                    "[chunk_embed] meeting {meeting_id}: no embedding model loaded; \
                     search via FTS branch (run embedder download to enable vectors)"
                );
            }
        }

        // Chain: diarization and extraction run after chunking, in parallel. A diarize
        // failure must not block extraction (Phase 2 degradation rule).
        let empty = serde_json::json!({});
        ctx.enqueue(kind::DIARIZE, Some(meeting_id), &empty).await?;
        ctx.enqueue(kind::EXTRACT, Some(meeting_id), &empty).await?;
        Ok(())
    }
}

pub struct DiarizeHandler;

#[async_trait]
impl JobHandler for DiarizeHandler {
    fn kind(&self) -> &'static str {
        kind::DIARIZE
    }

    async fn run(
        &self,
        ctx: &JobContext,
        meeting_id: Option<&str>,
        _payload: &serde_json::Value,
    ) -> anyhow::Result<()> {
        use crate::pipeline::diarization_commands::{app_handle, run_diarization_core, DiarizeError};

        let meeting_id = meeting_id.ok_or_else(|| anyhow::anyhow!("diarize requires a meeting_id"))?;

        // The runner owns no AppHandle; the diarize core reaches Tauri (model dir + event
        // emission) via the process-wide handle set at startup. If it is absent (should not
        // happen in a running app), degrade gracefully — segments stay unattributed.
        let Some(app) = app_handle() else {
            log::info!(
                "[diarize] meeting {meeting_id}: app handle unavailable; \
                 leaving segments unattributed (search/RAG unaffected)"
            );
            return Ok(());
        };

        // Diarization degrades safely: model/recording absent -> succeed with segments left
        // unattributed. Only genuine failures propagate (retryable). Because diarize is its
        // own job (chunk_embed already ran and enqueued extract in parallel), a failure here
        // can never block search or extraction.
        match run_diarization_core(&app, &ctx.pool, meeting_id).await {
            Ok(o) => {
                log::info!(
                    "[diarize] meeting {meeting_id}: {} speaker(s), {}/{} segment(s) attributed",
                    o.speaker_count,
                    o.assigned_segments,
                    o.total_segments
                );
                Ok(())
            }
            Err(DiarizeError::ModelsUnavailable) => {
                log::info!(
                    "[diarize] meeting {meeting_id}: diarization models unavailable; \
                     leaving segments unattributed (search/RAG unaffected)"
                );
                Ok(())
            }
            Err(DiarizeError::NoRecording) => {
                log::info!(
                    "[diarize] meeting {meeting_id}: no saved recording; \
                     leaving segments unattributed"
                );
                Ok(())
            }
            Err(DiarizeError::Other(e)) => Err(e),
        }
    }
}

pub struct ExtractHandler;

#[async_trait]
impl JobHandler for ExtractHandler {
    fn kind(&self) -> &'static str {
        kind::EXTRACT
    }

    async fn run(
        &self,
        ctx: &JobContext,
        meeting_id: Option<&str>,
        _payload: &serde_json::Value,
    ) -> anyhow::Result<()> {
        use crate::llm::{complete_routed, prompts, router::Scope, LlmError, Purpose};
        use crate::pipeline::extraction;

        let meeting_id = meeting_id.unwrap_or("<none>");
        let pool = &ctx.pool;

        // Load transcript with speaker labels where available.
        let segs: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT transcript, speaker FROM transcripts WHERE meeting_id = ? \
             ORDER BY COALESCE(audio_start_time, 0.0), rowid",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await?;
        if segs.is_empty() {
            log::info!("[extract] meeting {meeting_id}: no transcript; nothing to extract");
            return Ok(());
        }
        let transcript = segs
            .iter()
            .map(|(t, spk)| match spk {
                Some(s) if !s.is_empty() => format!("[{s}] {t}"),
                _ => t.clone(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        let title: String = sqlx::query_scalar("SELECT title FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await?
            .unwrap_or_default();

        let filled = prompts::fill(
            prompts::extract_v1(),
            &[("transcript", &transcript), ("meeting_title", &title), ("meeting_date", "")],
        );
        let system = "Верни только валидный JSON строго по инструкции ниже.";

        // Call the routed provider (privacy-guarded inside complete_routed). Retry once
        // on invalid JSON, per the plan. Extraction is optional: on a disabled purpose
        // or provider error we degrade (Ok) rather than failing the pipeline.
        let mut extraction = None;
        for attempt in 1..=2 {
            let raw = match complete_routed(
                pool,
                Purpose::Extract,
                Scope::SingleMeeting,
                transcript.len(),
                system,
                &filled,
            )
            .await
            {
                Ok(r) => r,
                Err(LlmError::Provider(e)) => {
                    log::warn!("[extract] meeting {meeting_id}: provider error, skipping: {e}");
                    return Ok(());
                }
                Err(e) => {
                    log::info!("[extract] meeting {meeting_id}: skipped ({e})");
                    return Ok(());
                }
            };
            match extraction::parse_and_validate(&raw) {
                Ok(x) => {
                    extraction = Some(x);
                    break;
                }
                Err(e) => log::warn!("[extract] meeting {meeting_id}: invalid JSON (attempt {attempt}): {e}"),
            }
        }

        let Some(extraction) = extraction else {
            log::warn!("[extract] meeting {meeting_id}: extraction failed after retry");
            return Ok(());
        };
        log::info!(
            "[extract] meeting {meeting_id}: extracted {} entities, {} action item(s)",
            extraction.entities.len(),
            extraction.action_items.len()
        );
        // TODO(Phase 3 persistence): resolve entities (extraction::resolve_entity) into
        // entities/pending_merges, map quotes to chunks, insert entity_mentions +
        // action_items. The extraction call itself (providers) is now wired end-to-end.
        Ok(())
    }
}

pub struct BackfillHandler;

#[async_trait]
impl JobHandler for BackfillHandler {
    fn kind(&self) -> &'static str {
        kind::BACKFILL
    }

    async fn run(
        &self,
        ctx: &JobContext,
        _meeting_id: Option<&str>,
        _payload: &serde_json::Value,
    ) -> anyhow::Result<()> {
        // Enqueue the pipeline for every meeting that has no chunks yet. Idempotent: the
        // deterministic chunker makes re-running chunk_embed safe, and the queue's bounded
        // concurrency rate-limits downstream LLM extraction (PLAN.md Phase 5).
        let meeting_ids: Vec<String> = sqlx::query_scalar(
            "SELECT m.id FROM meetings m \
             WHERE NOT EXISTS (SELECT 1 FROM chunks c WHERE c.meeting_id = m.id)",
        )
        .fetch_all(&ctx.pool)
        .await?;

        log::info!("[backfill] enqueuing pipeline for {} un-chunked meeting(s)", meeting_ids.len());
        for id in meeting_ids {
            ctx.enqueue(kind::CHUNK_EMBED, Some(&id), &serde_json::json!({})).await?;
        }
        Ok(())
    }
}
