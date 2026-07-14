//! Embedding-model management (PLAN.md Phase 1): download, status, and startup load of
//! the local sentence embedder. Model files live in `app_data_dir/models/embedding/`.
//!
//! Default model: `multilingual-e5-small` ONNX export (dim 384) from the Xenova mirror,
//! which ships `model.onnx` + `tokenizer.json`. Everything degrades gracefully — until the
//! model is present, search/RAG run on the FTS branch only.

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::pipeline::embedder;
use crate::state::AppState;

const MODEL_URL: &str =
    "https://huggingface.co/Xenova/multilingual-e5-small/resolve/main/onnx/model.onnx";
const TOKENIZER_URL: &str =
    "https://huggingface.co/Xenova/multilingual-e5-small/resolve/main/tokenizer.json";

fn embedding_model_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models")
        .join("embedding");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn model_present(dir: &Path) -> bool {
    dir.join("model.onnx").exists() && dir.join("tokenizer.json").exists()
}

#[derive(serde::Serialize, Clone)]
struct DownloadProgress {
    file: String,
    downloaded: u64,
    total: u64,
    percent: u8,
}

/// Stream a URL to `dest` (atomic via a `.part` temp), emitting `embedder-download-progress`.
/// No-op if `dest` already exists.
async fn download_file<R: Runtime>(
    app: &AppHandle<R>,
    url: &str,
    dest: &Path,
    label: &str,
) -> Result<(), String> {
    if dest.exists() {
        return Ok(());
    }
    let tmp = dest.with_extension("part");
    let resp = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("download {label}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("download {label}: HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);

    use std::io::Write;
    let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut last_pct: u8 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("download {label}: {e}"))?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if total > 0 {
            let pct = ((downloaded.saturating_mul(100)) / total) as u8;
            if pct != last_pct {
                last_pct = pct;
                let _ = app.emit(
                    "embedder-download-progress",
                    DownloadProgress {
                        file: label.to_string(),
                        downloaded,
                        total,
                        percent: pct,
                    },
                );
            }
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
    Ok(())
}

/// Report whether the model is downloaded and loaded.
#[tauri::command]
pub async fn embedder_status<R: Runtime>(app: AppHandle<R>) -> Result<serde_json::Value, String> {
    let dir = embedding_model_dir(&app)?;
    Ok(serde_json::json!({
        "model": "multilingual-e5-small",
        "dim": crate::vector::EMBEDDING_DIM,
        "model_present": model_present(&dir),
        "loaded": embedder::is_loaded(),
    }))
}

#[derive(Debug, serde::Serialize)]
pub struct IndexingStatus {
    pub indexable_meetings: i64,
    pub chunked_meetings: i64,
    pub chunks_total: i64,
    pub embeddings_done: i64,
    pub embeddings_pending: i64,
    pub embeddings_failed: i64,
    pub queued_jobs: i64,
    pub running_jobs: i64,
    pub unresolved_failed_jobs: i64,
    pub needs_repair: bool,
}

/// Observable archive-index health for Settings. Only indexing-related jobs are
/// included; optional diarization/extraction failures do not make search look broken.
#[tauri::command]
pub async fn indexing_status(state: tauri::State<'_, AppState>) -> Result<IndexingStatus, String> {
    let pool = state.db_manager.pool();
    let indexable_meetings: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM meetings m WHERE EXISTS ( \
           SELECT 1 FROM transcripts t \
           WHERE t.meeting_id=m.id AND length(trim(t.transcript)) > 0 \
         )",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    let chunked_meetings: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT meeting_id) FROM chunks")
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;
    let (chunks_total, embeddings_done, embeddings_pending, embeddings_failed): (
        i64,
        i64,
        i64,
        i64,
    ) = sqlx::query_as(
        "SELECT COUNT(*), \
                COALESCE(SUM(embedding_status='done'), 0), \
                COALESCE(SUM(embedding_status='pending'), 0), \
                COALESCE(SUM(embedding_status='failed'), 0) \
         FROM chunks",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    let (queued_jobs, running_jobs): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(status='queued'), 0), COALESCE(SUM(status='running'), 0) \
         FROM jobs WHERE kind IN ('chunk_embed', 'embedding_repair', 'backfill')",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    let unresolved_failed_jobs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM jobs j \
         WHERE j.kind IN ('chunk_embed', 'embedding_repair', 'backfill') \
           AND j.status='failed' \
           AND j.id = ( \
             SELECT MAX(j2.id) FROM jobs j2 \
             WHERE j2.kind=j.kind AND j2.meeting_id IS j.meeting_id \
           )",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(IndexingStatus {
        indexable_meetings,
        chunked_meetings,
        chunks_total,
        embeddings_done,
        embeddings_pending,
        embeddings_failed,
        queued_jobs,
        running_jobs,
        unresolved_failed_jobs,
        needs_repair: chunked_meetings < indexable_meetings
            || embeddings_pending > 0
            || embeddings_failed > 0,
    })
}

/// Download the embedding model (tokenizer first, then the ONNX weights) and load it.
/// Emits `embedder-download-progress` while downloading and `embedder-ready` on success.
#[tauri::command]
pub async fn embedder_download_model<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let dir = embedding_model_dir(&app)?;
    download_file(
        &app,
        TOKENIZER_URL,
        &dir.join("tokenizer.json"),
        "tokenizer.json",
    )
    .await?;
    download_file(&app, MODEL_URL, &dir.join("model.onnx"), "model.onnx").await?;

    let dir_for_load = dir.clone();
    tokio::task::spawn_blocking(move || embedder::load_global(dir_for_load))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    let _ = app.emit("embedder-ready", ());
    let outcome = crate::jobs::store::enqueue_unique(
        state.db_manager.pool(),
        crate::jobs::kind::BACKFILL,
        None,
        &serde_json::json!({ "reason": "embedder_ready" }),
    )
    .await
    .map_err(|e| e.to_string())?;
    log::info!(
        "embedding model ready; archive repair job {} ({})",
        outcome.id,
        if outcome.created {
            "queued"
        } else {
            "already active"
        }
    );
    log::info!("embedding model downloaded and loaded");
    Ok(())
}

/// Load the embedder at startup if its files are already present. Non-blocking.
pub async fn init_embedder_at_startup<R: Runtime>(app: &AppHandle<R>) {
    let dir = match embedding_model_dir(app) {
        Ok(d) => d,
        Err(e) => {
            log::warn!("could not resolve embedding model dir: {e}");
            return;
        }
    };
    if model_present(&dir) {
        let loaded = tokio::task::spawn_blocking(move || {
            if let Err(e) = embedder::load_global(dir) {
                log::warn!("failed to load embedder at startup: {e}");
                false
            } else {
                true
            }
        })
        .await
        .unwrap_or(false);
        if loaded {
            if let Some(state) = app.try_state::<AppState>() {
                match crate::jobs::store::enqueue_unique(
                    state.db_manager.pool(),
                    crate::jobs::kind::BACKFILL,
                    None,
                    &serde_json::json!({ "reason": "startup" }),
                )
                .await
                {
                    Ok(outcome) if outcome.created => {
                        log::info!("queued archive index repair after embedder startup")
                    }
                    Ok(_) => {}
                    Err(e) => log::warn!("failed to queue startup archive repair: {e}"),
                }
            }
        }
    } else {
        log::info!("embedding model not present; search/RAG run FTS-only until it is downloaded");
    }
}
