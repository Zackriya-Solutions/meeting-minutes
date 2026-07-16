//! Embedding-model management (PLAN.md Phase 1): download, status, and startup load of
//! the local sentence embedder. Model files live in `app_data_dir/models/embedding/`.
//!
//! Default model: `multilingual-e5-small` ONNX export (dim 384) from the Xenova mirror,
//! which ships `model.onnx` + `tokenizer.json`. Everything degrades gracefully — until the
//! model is present, search/RAG run on the FTS branch only.

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::pipeline::embedder::{self, EmbedderConfig, EmbedderKind};
use crate::state::AppState;

const E5_MODEL_URL: &str =
    "https://huggingface.co/Xenova/multilingual-e5-small/resolve/main/onnx/model.onnx";
const E5_TOKENIZER_URL: &str =
    "https://huggingface.co/Xenova/multilingual-e5-small/resolve/main/tokenizer.json";
const FRIDA_REVISION: &str = "d3a04c3460f6b16f2f8f0859b2b67a46cb388558";

fn frida_model_url(file: &str) -> String {
    format!(
        "https://huggingface.co/geologist387/FRIDA-transformed/resolve/{FRIDA_REVISION}/\
         onnx/frida-onnx/{file}"
    )
}

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

fn model_dir(base: &Path, kind: EmbedderKind) -> PathBuf {
    match kind {
        EmbedderKind::MultilingualE5Small => base.to_path_buf(),
        EmbedderKind::Frida => base.join("frida"),
    }
}

fn model_present(base: &Path, kind: EmbedderKind) -> bool {
    EmbedderConfig::for_kind(model_dir(base, kind), kind).is_available()
}

async fn selected_kind(pool: &sqlx::SqlitePool) -> EmbedderKind {
    let selected: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_settings_kv WHERE key='embedding.model'")
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    EmbedderKind::parse(selected.as_deref().unwrap_or("multilingual-e5-small"))
}

async fn persist_selected_kind(pool: &sqlx::SqlitePool, kind: EmbedderKind) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO app_settings_kv(key,value,updated_at) VALUES('embedding.model',?,CURRENT_TIMESTAMP) \
         ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=CURRENT_TIMESTAMP",
    )
    .bind(kind.id())
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
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
pub async fn embedder_status<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let base = embedding_model_dir(&app)?;
    let kind = selected_kind(state.db_manager.pool()).await;
    Ok(serde_json::json!({
        "model": kind.id(),
        "dim": kind.dim(),
        "model_present": model_present(&base, kind),
        "loaded": embedder::is_loaded(),
        "available_models": [
            {
                "id": "multilingual-e5-small",
                "name": "Multilingual E5 Small",
                "dim": crate::vector::EMBEDDING_DIM,
                "download_mb": 470,
                "present": model_present(&base, EmbedderKind::MultilingualE5Small),
            },
            {
                "id": "frida",
                "name": "FRIDA",
                "dim": crate::vector::FRIDA_EMBEDDING_DIM,
                "download_mb": 3300,
                "present": model_present(&base, EmbedderKind::Frida),
                "revision": FRIDA_REVISION,
            }
        ],
    }))
}

async fn activate_model<R: Runtime>(
    app: &AppHandle<R>,
    pool: &sqlx::SqlitePool,
    kind: EmbedderKind,
) -> Result<bool, String> {
    let base = embedding_model_dir(app)?;
    if !model_present(&base, kind) {
        return Ok(false);
    }

    let dir = model_dir(&base, kind);
    let candidate = tokio::task::spawn_blocking(move || embedder::load_kind(dir, kind))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    let _switch_guard = embedder::model_index_write_guard().await;
    let previous_kind = selected_kind(pool).await;
    persist_selected_kind(pool, kind).await?;
    let index_ready =
        crate::vector::ensure_chunk_embeddings_table_for_dim(pool, kind.dim()).await;
    if !matches!(index_ready, Ok(true)) {
        if let Err(rollback_error) = persist_selected_kind(pool, previous_kind).await {
            return Err(format!(
                "vector index is unavailable; failed to restore embedding model selection: \
                 {rollback_error}"
            ));
        }
        return Err(match index_ready {
            Ok(false) => "vector index is unavailable; model selection was not changed".to_string(),
            Err(error) => error.to_string(),
            Ok(true) => unreachable!(),
        });
    }
    embedder::install_global(candidate);
    Ok(true)
}

#[tauri::command]
pub async fn embedder_select_model<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    model: String,
) -> Result<(), String> {
    let kind = EmbedderKind::parse(&model);
    let loaded = activate_model(&app, state.db_manager.pool(), kind).await?;
    if loaded {
        let _ = crate::jobs::store::enqueue_unique(
            state.db_manager.pool(),
            crate::jobs::kind::BACKFILL,
            None,
            &serde_json::json!({ "reason": "embedding_model_changed", "model": kind.id() }),
        )
        .await
        .map_err(|e| e.to_string())?;
        let _ = app.emit("embedder-ready", ());
    }
    Ok(())
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
    model: Option<String>,
) -> Result<(), String> {
    let kind = model
        .as_deref()
        .map(EmbedderKind::parse)
        .unwrap_or_else(|| EmbedderKind::parse("multilingual-e5-small"));
    let base = embedding_model_dir(&app)?;
    let dir = model_dir(&base, kind);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    match kind {
        EmbedderKind::MultilingualE5Small => {
            download_file(
                &app,
                E5_TOKENIZER_URL,
                &dir.join("tokenizer.json"),
                "tokenizer.json",
            )
            .await?;
            download_file(&app, E5_MODEL_URL, &dir.join("model.onnx"), "model.onnx").await?;
        }
        EmbedderKind::Frida => {
            let tokenizer_url = frida_model_url("tokenizer.json");
            download_file(
                &app,
                &tokenizer_url,
                &dir.join("tokenizer.json"),
                "tokenizer.json",
            )
            .await?;
            let model_url = frida_model_url("FRIDA.onnx");
            download_file(&app, &model_url, &dir.join("FRIDA.onnx"), "FRIDA.onnx").await?;
            let model_data_url = frida_model_url("FRIDA.onnx.data");
            download_file(
                &app,
                &model_data_url,
                &dir.join("FRIDA.onnx.data"),
                "FRIDA.onnx.data",
            )
            .await?;
        }
    }

    activate_model(&app, state.db_manager.pool(), kind).await?;

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
    log::info!("embedding model {} downloaded and loaded", kind.id());
    Ok(())
}

/// Load the embedder at startup if its files are already present. Non-blocking.
pub async fn init_embedder_at_startup<R: Runtime>(app: &AppHandle<R>) {
    let base = match embedding_model_dir(app) {
        Ok(d) => d,
        Err(e) => {
            log::warn!("could not resolve embedding model dir: {e}");
            return;
        }
    };
    let Some(state) = app.try_state::<AppState>() else {
        let kind = EmbedderKind::MultilingualE5Small;
        if model_present(&base, kind) {
            let dir = model_dir(&base, kind);
            match tokio::task::spawn_blocking(move || embedder::load_kind(dir, kind)).await {
                Ok(Ok(candidate)) => {
                    let _switch_guard = embedder::model_index_write_guard().await;
                    embedder::install_global(candidate);
                }
                Ok(Err(e)) => log::warn!("failed to load embedder before database setup: {e}"),
                Err(e) => log::warn!("embedding startup task failed: {e}"),
            }
        } else {
            log::info!(
                "embedding model not present; search/RAG run FTS-only until it is downloaded"
            );
        }
        return;
    };
    let kind = selected_kind(state.db_manager.pool()).await;
    if model_present(&base, kind) {
        let dir = model_dir(&base, kind);
        let candidate =
            match tokio::task::spawn_blocking(move || embedder::load_kind(dir, kind)).await {
                Ok(Ok(candidate)) => candidate,
                Ok(Err(e)) => {
                    log::warn!("failed to load embedder at startup: {e}");
                    return;
                }
                Err(e) => {
                    log::warn!("embedding startup task failed: {e}");
                    return;
                }
            };
        let switch_guard = embedder::model_index_write_guard().await;
        match crate::vector::ensure_chunk_embeddings_table_for_dim(
            state.db_manager.pool(),
            kind.dim(),
        )
        .await
        {
            Ok(true) => {}
            Ok(false) => {
                log::warn!(
                    "vector index unavailable; not activating {} at startup",
                    kind.id()
                );
                return;
            }
            Err(e) => {
                log::warn!("failed to prepare vector table for {}: {e}", kind.id());
                return;
            }
        }
        embedder::install_global(candidate);
        drop(switch_guard);
        match crate::jobs::store::enqueue_unique(
            state.db_manager.pool(),
            crate::jobs::kind::BACKFILL,
            None,
            &serde_json::json!({ "reason": "startup", "model": kind.id() }),
        )
        .await
        {
            Ok(outcome) if outcome.created => {
                log::info!("queued archive index repair after embedder startup")
            }
            Ok(_) => {}
            Err(e) => log::warn!("failed to queue startup archive repair: {e}"),
        }
    } else {
        log::info!("embedding model not present; search/RAG run FTS-only until it is downloaded");
    }
}
