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
use std::path::PathBuf;

use super::{kind, JobContext, JobHandler};
use crate::pipeline::chunker::{approx_token_count, chunk_segments, ChunkConfig, Segment};

pub struct ChunkEmbedHandler;

async fn enqueue_analysis_jobs(ctx: &JobContext, meeting_id: &str) -> anyhow::Result<()> {
    let payload = serde_json::json!({
        "run_analysis": true,
        "source": "post_meeting",
    });
    ctx.enqueue_unique(kind::DIARIZE, Some(meeting_id), &payload)
        .await?;
    ctx.enqueue_unique(kind::EXTRACT, Some(meeting_id), &payload)
        .await?;
    Ok(())
}

#[async_trait]
impl JobHandler for ChunkEmbedHandler {
    fn kind(&self) -> &'static str {
        kind::CHUNK_EMBED
    }

    async fn run(
        &self,
        ctx: &JobContext,
        meeting_id: Option<&str>,
        payload: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let meeting_id =
            meeting_id.ok_or_else(|| anyhow::anyhow!("chunk_embed requires a meeting_id"))?;
        let pool = &ctx.pool;
        // Archive repair must only build the search index. Diarization and LLM
        // extraction read whole recordings/transcripts and are appropriate only for
        // the explicit post-meeting pipeline. Defaulting old/untagged jobs to false
        // also makes queues created by earlier auto-backfill builds safe after upgrade.
        let run_analysis = payload
            .get("run_analysis")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let source = payload
            .get("source")
            .and_then(serde_json::Value::as_str);
        if !run_analysis && !matches!(source, Some("archive_backfill")) {
            log::info!(
                "[chunk_embed] meeting {meeting_id}: skipped stale/untagged background job"
            );
            return Ok(());
        }

        let indexing_allowed: Option<i64> = sqlx::query_scalar(
            "SELECT indexing_allowed FROM meetings WHERE id = ?",
        )
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?;
        if indexing_allowed == Some(0) {
            let old_ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM chunks WHERE meeting_id = ?")
                .bind(meeting_id)
                .fetch_all(pool)
                .await?;
            for id in old_ids {
                let _ = sqlx::query("DELETE FROM chunk_embeddings WHERE chunk_id = ?")
                    .bind(id)
                    .execute(pool)
                    .await;
            }
            sqlx::query("DELETE FROM chunks WHERE meeting_id = ?")
                .bind(meeting_id)
                .execute(pool)
                .await?;
            log::info!("[chunk_embed] meeting {meeting_id}: indexing disabled by memory privacy policy");
            return if run_analysis {
                enqueue_analysis_jobs(ctx, meeting_id).await
            } else {
                Ok(())
            };
        }

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

        let mut embedding_needs_repair = false;
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
            log::info!(
                "[chunk_embed] meeting {meeting_id}: created {} chunk(s)",
                chunk_ids.len()
            );

            // Embedding step: embed each chunk and write to the vec0 table. If no model is
            // loaded (or sqlite-vec is unavailable), chunks stay embedding_status='pending'
            // and search runs on the FTS branch only — never fatal.
            let _model_index_guard = crate::pipeline::embedder::model_index_read_guard().await;
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
                                Err(e) => {
                                    embedding_needs_repair = true;
                                    let _ = sqlx::query(
                                        "UPDATE chunks SET embedding_status='failed' WHERE id=?",
                                    )
                                    .bind(*id)
                                    .execute(pool)
                                    .await;
                                    log::warn!("[chunk_embed] upsert failed for chunk {id}: {e}");
                                }
                            }
                        }
                        log::info!(
                            "[chunk_embed] meeting {meeting_id}: embedded {embedded} chunk(s)"
                        );
                    }
                    Some(Ok(_)) => {
                        embedding_needs_repair = true;
                        log::warn!("[chunk_embed] embedding count mismatch; marking chunks failed");
                        let _ = sqlx::query(
                            "UPDATE chunks SET embedding_status='failed' \
                             WHERE meeting_id=? AND embedding_status != 'done'",
                        )
                        .bind(meeting_id)
                        .execute(pool)
                        .await;
                    }
                    Some(Err(e)) => {
                        embedding_needs_repair = true;
                        log::warn!("[chunk_embed] embedding failed: {e}");
                        let _ = sqlx::query(
                            "UPDATE chunks SET embedding_status='failed' \
                             WHERE meeting_id=? AND embedding_status != 'done'",
                        )
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

        if embedding_needs_repair {
            ctx.enqueue_unique(
                kind::EMBEDDING_REPAIR,
                Some(meeting_id),
                &serde_json::json!({
                    "reason": "chunk_embed_partial_failure",
                    "source": source.unwrap_or(if run_analysis {
                        "post_meeting"
                    } else {
                        "archive_backfill"
                    }),
                }),
            )
            .await?;
        }

        if run_analysis {
            // The explicit post-meeting path chains optional analysis after search
            // indexing. Archive backfills intentionally stop here.
            enqueue_analysis_jobs(ctx, meeting_id).await
        } else {
            log::info!(
                "[chunk_embed] meeting {meeting_id}: index-only job complete; optional analysis skipped"
            );
            Ok(())
        }
    }
}

/// Repair vector embeddings without deleting/recreating chunks. Keeping chunk
/// ids stable preserves citations and avoids unnecessary FTS trigger churn.
pub struct EmbeddingRepairHandler;

#[async_trait]
impl JobHandler for EmbeddingRepairHandler {
    fn kind(&self) -> &'static str {
        kind::EMBEDDING_REPAIR
    }

    async fn run(
        &self,
        ctx: &JobContext,
        meeting_id: Option<&str>,
        payload: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let meeting_id =
            meeting_id.ok_or_else(|| anyhow::anyhow!("embedding_repair requires a meeting_id"))?;
        if !matches!(
            payload
                .get("source")
                .and_then(serde_json::Value::as_str),
            Some("post_meeting" | "archive_backfill")
        ) {
            log::info!(
                "[embedding_repair] meeting {meeting_id}: skipped stale/untagged background job"
            );
            return Ok(());
        }
        let _model_index_guard = crate::pipeline::embedder::model_index_read_guard().await;
        if !crate::pipeline::embedder::is_loaded() {
            log::info!(
                "[embedding_repair] meeting {meeting_id}: model not loaded; leaving chunks pending"
            );
            return Ok(());
        }

        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT id, text FROM chunks \
             WHERE meeting_id = ? AND embedding_status != 'done' ORDER BY id",
        )
        .bind(meeting_id)
        .fetch_all(&ctx.pool)
        .await?;
        if rows.is_empty() {
            return Ok(());
        }

        let texts = rows.iter().map(|(_, text)| text.clone()).collect();
        let vectors = match crate::pipeline::embedder::embed_passages(texts).await {
            Some(Ok(vectors)) if vectors.len() == rows.len() => vectors,
            Some(Ok(_)) => {
                sqlx::query(
                    "UPDATE chunks SET embedding_status='failed' \
                     WHERE meeting_id=? AND embedding_status != 'done'",
                )
                .bind(meeting_id)
                .execute(&ctx.pool)
                .await?;
                anyhow::bail!("embedding count mismatch");
            }
            Some(Err(e)) => {
                sqlx::query(
                    "UPDATE chunks SET embedding_status='failed' \
                     WHERE meeting_id=? AND embedding_status != 'done'",
                )
                .bind(meeting_id)
                .execute(&ctx.pool)
                .await?;
                return Err(anyhow::anyhow!(e));
            }
            None => anyhow::bail!("embedding model became unavailable"),
        };

        let mut failures = Vec::new();
        for ((chunk_id, _), vector) in rows.iter().zip(vectors) {
            match crate::vector::upsert_embedding(&ctx.pool, *chunk_id, &vector).await {
                Ok(()) => {
                    sqlx::query("UPDATE chunks SET embedding_status='done' WHERE id=?")
                        .bind(*chunk_id)
                        .execute(&ctx.pool)
                        .await?;
                }
                Err(e) => {
                    sqlx::query("UPDATE chunks SET embedding_status='failed' WHERE id=?")
                        .bind(*chunk_id)
                        .execute(&ctx.pool)
                        .await?;
                    failures.push(format!("chunk {chunk_id}: {e}"));
                }
            }
        }

        if failures.is_empty() {
            log::info!(
                "[embedding_repair] meeting {meeting_id}: repaired {} chunk(s)",
                rows.len()
            );
            Ok(())
        } else {
            anyhow::bail!(
                "{} embedding write(s) failed: {}",
                failures.len(),
                failures.join("; ")
            )
        }
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
        payload: &serde_json::Value,
    ) -> anyhow::Result<()> {
        use crate::pipeline::diarization_commands::{
            app_handle, run_diarization_core, DiarizeError,
        };

        let meeting_id =
            meeting_id.ok_or_else(|| anyhow::anyhow!("diarize requires a meeting_id"))?;
        if payload
            .get("run_analysis")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        {
            log::info!(
                "[diarize] meeting {meeting_id}: skipped stale/untagged background analysis job"
            );
            return Ok(());
        }

        // Per-meeting opt-out (in-meeting control pill): diarization_enabled = 0 skips the
        // automatic job. The manual `diarize_meeting` command bypasses this — an explicit
        // "Detect speakers" click always runs.
        let prefs =
            crate::database::repositories::meeting::MeetingsRepository::get_diarization_prefs(
                &ctx.pool, meeting_id,
            )
            .await?;
        if prefs.and_then(|(enabled, _)| enabled) == Some(false) {
            log::info!(
                "[diarize] meeting {meeting_id}: speaker ID disabled for this meeting; \
                 leaving segments unattributed"
            );
            return Ok(());
        }

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
        payload: &serde_json::Value,
    ) -> anyhow::Result<()> {
        use crate::llm::{complete_routed, prompts, router::Scope, LlmError, Purpose};
        use crate::pipeline::extraction;

        let meeting_id = meeting_id.unwrap_or("<none>");
        if payload
            .get("run_analysis")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        {
            log::info!(
                "[extract] meeting {meeting_id}: skipped stale/untagged background analysis job"
            );
            return Ok(());
        }
        let pool = &ctx.pool;

        let cloud_processing_allowed: Option<i64> = sqlx::query_scalar(
            "SELECT cloud_processing_allowed FROM meetings WHERE id = ?",
        )
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?;
        if cloud_processing_allowed == Some(0) {
            log::info!(
                "[extract] meeting {meeting_id}: cloud extraction disabled by memory privacy policy"
            );
            return Ok(());
        }

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
            &[
                ("transcript", &transcript),
                ("meeting_title", &title),
                ("meeting_date", ""),
            ],
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
                Err(e) => log::warn!(
                    "[extract] meeting {meeting_id}: invalid JSON (attempt {attempt}): {e}"
                ),
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
        let persisted = crate::pipeline::extraction_persistence::persist_extraction(
            pool,
            meeting_id,
            &extraction,
        )
        .await?;
        log::info!(
            "[extract] meeting {meeting_id}: persisted {} entities, {} mentions, {} reviews, {} actions",
            persisted.entities_created,
            persisted.mentions_created,
            persisted.pending_merges_created,
            persisted.action_items_created,
        );
        Ok(())
    }
}

pub struct BackfillHandler;

pub struct AudioIdentityBackfillHandler;

#[async_trait]
impl JobHandler for AudioIdentityBackfillHandler {
    fn kind(&self) -> &'static str {
        kind::AUDIO_IDENTITY_BACKFILL
    }

    async fn run(
        &self,
        ctx: &JobContext,
        meeting_id: Option<&str>,
        _payload: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let meeting_id = meeting_id
            .ok_or_else(|| anyhow::anyhow!("audio_identity_backfill requires a meeting_id"))?;
        let folder_path: Option<String> =
            sqlx::query_scalar("SELECT folder_path FROM meetings WHERE id = ?")
                .bind(meeting_id)
                .fetch_optional(&ctx.pool)
                .await?
                .flatten();
        let Some(folder_path) = folder_path else {
            log::info!("[audio_identity_backfill] meeting {meeting_id} has no recording folder");
            return Ok(());
        };

        let folder = PathBuf::from(folder_path);
        let audio_path = match crate::audio::retranscription::find_audio_file(&folder) {
            Ok(path) => path,
            Err(error) => {
                log::warn!(
                    "[audio_identity_backfill] meeting {meeting_id} has no readable audio: {error}"
                );
                return Ok(());
            }
        };
        let (sha256, byte_size) = tokio::task::spawn_blocking(move || {
            let byte_size = std::fs::metadata(&audio_path)?.len();
            let sha256 = crate::audio::import::sha256_file(&audio_path)?;
            Ok::<_, anyhow::Error>((sha256, byte_size))
        })
        .await
        .map_err(|error| anyhow::anyhow!("audio identity hash task failed: {error}"))??;

        let mut tx = ctx.pool.begin().await?;
        let registration =
            crate::database::repositories::audio_identity::register_backfilled_identity(
                &mut tx, meeting_id, &sha256, byte_size, None,
            )
            .await?;
        tx.commit().await?;
        log::info!("[audio_identity_backfill] meeting {meeting_id} registered as {registration:?}");
        Ok(())
    }
}

#[async_trait]
impl JobHandler for BackfillHandler {
    fn kind(&self) -> &'static str {
        kind::BACKFILL
    }

    async fn run(
        &self,
        ctx: &JobContext,
        _meeting_id: Option<&str>,
        payload: &serde_json::Value,
    ) -> anyhow::Result<()> {
        if payload
            .get("reason")
            .and_then(serde_json::Value::as_str)
            == Some("startup")
        {
            // Builds before this policy change may have persisted an automatic startup
            // backfill. Mark it done without fanning out work across an imported archive.
            log::info!("[backfill] skipped legacy automatic startup archive repair");
            return Ok(());
        }
        // Only meetings with transcript content are indexable. Empty recordings remain
        // intentionally absent instead of being enqueued on every repair pass.
        let meeting_ids: Vec<String> = sqlx::query_scalar(
            "SELECT m.id FROM meetings m \
             WHERE EXISTS ( \
               SELECT 1 FROM transcripts t \
               WHERE t.meeting_id = m.id AND length(trim(t.transcript)) > 0 \
             ) \
             AND NOT EXISTS (SELECT 1 FROM chunks c WHERE c.meeting_id = m.id)",
        )
        .fetch_all(&ctx.pool)
        .await?;

        let mut chunk_jobs = 0usize;
        let reason = payload
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("manual_repair");
        for id in meeting_ids {
            if ctx
                .enqueue_unique(
                    kind::CHUNK_EMBED,
                    Some(&id),
                    &serde_json::json!({
                        "run_analysis": false,
                        "source": "archive_backfill",
                        "reason": reason,
                    }),
                )
                .await?
                .created
            {
                chunk_jobs += 1;
            }
        }

        // If the vec0 table was recreated after corruption or manual cleanup,
        // reconcile authoritative chunk status before selecting repairs.
        let vector_table_exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master \
             WHERE type='table' AND name='chunk_embeddings')",
        )
        .fetch_one(&ctx.pool)
        .await?;
        if vector_table_exists != 0 {
            match sqlx::query(
                "UPDATE chunks SET embedding_status='pending' \
                 WHERE embedding_status='done' AND NOT EXISTS ( \
                   SELECT 1 FROM chunk_embeddings e WHERE e.chunk_id=chunks.id \
                 )",
            )
            .execute(&ctx.pool)
            .await
            {
                Ok(result) if result.rows_affected() > 0 => log::warn!(
                    "[backfill] found {} chunk(s) missing vector rows; marked for repair",
                    result.rows_affected()
                ),
                Ok(_) => {}
                Err(e) => log::warn!("[backfill] vector integrity check failed: {e}"),
            }
        }

        let mut repair_jobs = 0usize;
        if crate::pipeline::embedder::is_loaded() {
            let repair_ids: Vec<String> = sqlx::query_scalar(
                "SELECT DISTINCT meeting_id FROM chunks \
                 WHERE embedding_status != 'done' ORDER BY meeting_id",
            )
            .fetch_all(&ctx.pool)
            .await?;
            for id in repair_ids {
                if ctx
                    .enqueue_unique(
                        kind::EMBEDDING_REPAIR,
                        Some(&id),
                        &serde_json::json!({
                            "source": "archive_backfill",
                            "reason": reason,
                        }),
                    )
                    .await?
                    .created
                {
                    repair_jobs += 1;
                }
            }
        }

        log::info!(
            "[backfill] queued {chunk_jobs} chunk job(s) and {repair_jobs} embedding repair job(s)"
        );
        Ok(())
    }
}
