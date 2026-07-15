//! Sequential, resumable Standup V2 corpus runs for local evaluation.
//!
//! The runner accepts explicit meeting IDs, never discovers or uploads a corpus on its own, and
//! writes only run metadata to its report. Provider privacy policy is still enforced by the
//! ordinary summary service.

use crate::database::repositories::{
    summary::SummaryProcessesRepository, transcript_chunk::TranscriptChunksRepository,
};
use crate::state::AppState;
use crate::summary::service::SummaryService;
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Runtime};

const MAX_CORPUS_MEETINGS: usize = 50;
const TEMPLATE_ID: &str = "daily_standup";
static CORPUS_RUN_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

struct CorpusRunGuard;

impl CorpusRunGuard {
    fn acquire() -> Result<Self, String> {
        CORPUS_RUN_IN_PROGRESS
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "A standup corpus run is already in progress".to_string())?;
        Ok(Self)
    }
}

impl Drop for CorpusRunGuard {
    fn drop(&mut self) {
        CORPUS_RUN_IN_PROGRESS.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StandupCorpusRunItem {
    pub meeting_id: String,
    pub title: String,
    pub status: String,
    pub processing_time_ms: u64,
    pub chunk_count: i64,
    pub extracted_record_count: i64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StandupCorpusRunReport {
    pub schema_version: String,
    pub started_at: String,
    pub completed_at: String,
    pub provider: String,
    pub model: String,
    pub template_id: String,
    pub summary_language: Option<String>,
    pub requested: usize,
    pub completed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub items: Vec<StandupCorpusRunItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StandupCorpusRunProgress {
    pub current: usize,
    pub total: usize,
    pub meeting_id: String,
    pub title: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StandupCorpusRunStarted {
    pub message: String,
}

fn normalized_ids(meeting_ids: Vec<String>) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let ids = meeting_ids
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && seen.insert(value.clone()))
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Err("At least one meeting ID is required".to_string());
    }
    if ids.len() > MAX_CORPUS_MEETINGS {
        return Err(format!(
            "A corpus run is limited to {MAX_CORPUS_MEETINGS} meetings"
        ));
    }
    Ok(ids)
}

fn has_standup_v2(result: Option<&str>) -> bool {
    result
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .and_then(|value| value.get("standup_v2").cloned())
        .and_then(|value| value.get("schema_version").cloned())
        .and_then(|value| value.as_str().map(str::to_string))
        .as_deref()
        == Some("standup_v2")
}

fn bounded_error(value: Option<String>) -> Option<String> {
    value.map(|value| {
        value
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(500)
            .collect()
    })
}

async fn meeting_input(pool: &SqlitePool, meeting_id: &str) -> Result<(String, String), String> {
    let title: Option<String> = sqlx::query_scalar("SELECT title FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?;
    let title = title.ok_or_else(|| format!("Meeting not found: {meeting_id}"))?;
    let segments: Vec<(String, Option<f64>, String)> = sqlx::query_as(
        "SELECT transcript, audio_start_time, timestamp FROM transcripts \
         WHERE meeting_id = ? AND trim(transcript) != '' \
         ORDER BY CASE WHEN audio_start_time IS NULL THEN 1 ELSE 0 END, audio_start_time, timestamp, id",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    if segments.is_empty() {
        return Err("Meeting has no non-empty transcript".to_string());
    }
    let fallback = segments
        .into_iter()
        .map(|(text, audio_start_time, timestamp)| {
            let prefix = match audio_start_time {
                Some(seconds) => {
                    let seconds = seconds.max(0.0).floor() as i64;
                    format!("[{:02}:{:02}]", seconds / 60, seconds % 60)
                }
                None => timestamp,
            };
            format!("{prefix} {text}")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let transcript = crate::summary::transcript_labeling::build_speaker_labeled_transcript(
        pool, meeting_id, fallback,
    )
    .await;
    Ok((title, transcript))
}

fn write_report(path: &Path, report: &StandupCorpusRunReport) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(report).map_err(|error| error.to_string())?;
    std::fs::write(&temp, bytes).map_err(|error| error.to_string())?;
    std::fs::rename(&temp, path).map_err(|error| error.to_string())
}

#[allow(clippy::too_many_arguments)]
pub async fn run_standup_corpus<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    meeting_ids: Vec<String>,
    provider: String,
    model: String,
    summary_language: Option<String>,
    overwrite: bool,
    report_path: Option<PathBuf>,
) -> Result<StandupCorpusRunReport, String> {
    let guard = CorpusRunGuard::acquire()?;
    run_standup_corpus_inner(
        app,
        pool,
        meeting_ids,
        provider,
        model,
        summary_language,
        overwrite,
        report_path,
        guard,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run_standup_corpus_inner<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    meeting_ids: Vec<String>,
    provider: String,
    model: String,
    summary_language: Option<String>,
    overwrite: bool,
    report_path: Option<PathBuf>,
    _guard: CorpusRunGuard,
) -> Result<StandupCorpusRunReport, String> {
    let meeting_ids = normalized_ids(meeting_ids)?;
    let provider = provider.trim().to_string();
    let model = model.trim().to_string();
    if provider.is_empty() || model.is_empty() {
        return Err("Provider and model are required".to_string());
    }
    let started_at = chrono::Utc::now().to_rfc3339();
    let mut items = Vec::with_capacity(meeting_ids.len());

    for (index, meeting_id) in meeting_ids.iter().enumerate() {
        let (title, transcript) = match meeting_input(&pool, meeting_id).await {
            Ok(value) => value,
            Err(error) => {
                items.push(StandupCorpusRunItem {
                    meeting_id: meeting_id.clone(),
                    title: String::new(),
                    status: "failed".to_string(),
                    processing_time_ms: 0,
                    chunk_count: 0,
                    extracted_record_count: 0,
                    error: Some(error),
                });
                continue;
            }
        };
        let _ = app.emit(
            "standup-corpus-run-progress",
            StandupCorpusRunProgress {
                current: index + 1,
                total: meeting_ids.len(),
                meeting_id: meeting_id.clone(),
                title: title.clone(),
                state: "processing".to_string(),
            },
        );

        let existing = SummaryProcessesRepository::get_summary_data(&pool, meeting_id)
            .await
            .map_err(|error| error.to_string())?;
        if !overwrite
            && existing.as_ref().is_some_and(|row| {
                row.status == "completed" && has_standup_v2(row.result.as_deref())
            })
        {
            items.push(StandupCorpusRunItem {
                meeting_id: meeting_id.clone(),
                title,
                status: "skipped".to_string(),
                processing_time_ms: 0,
                chunk_count: existing
                    .as_ref()
                    .map(|row| row.chunk_count)
                    .unwrap_or_default(),
                extracted_record_count: 0,
                error: None,
            });
            continue;
        }

        SummaryProcessesRepository::create_or_reset_process(&pool, meeting_id)
            .await
            .map_err(|error| error.to_string())?;
        TranscriptChunksRepository::save_transcript_data(
            &pool,
            meeting_id,
            &transcript,
            &provider,
            &model,
            40_000,
            1_000,
        )
        .await
        .map_err(|error| error.to_string())?;
        SummaryService::process_transcript_background(
            app.clone(),
            pool.clone(),
            meeting_id.clone(),
            transcript,
            provider.clone(),
            model.clone(),
            String::new(),
            TEMPLATE_ID.to_string(),
            summary_language.clone(),
        )
        .await;

        let outcome = SummaryProcessesRepository::get_summary_data(&pool, meeting_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Summary process disappeared after generation".to_string())?;
        let extracted_record_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM standup_records WHERE meeting_id = ?")
                .bind(meeting_id)
                .fetch_one(&pool)
                .await
                .unwrap_or(0);
        let completed = outcome.status == "completed" && has_standup_v2(outcome.result.as_deref());
        items.push(StandupCorpusRunItem {
            meeting_id: meeting_id.clone(),
            title,
            status: if completed { "completed" } else { "failed" }.to_string(),
            processing_time_ms: (outcome.processing_time.max(0.0) * 1_000.0).round() as u64,
            chunk_count: outcome.chunk_count,
            extracted_record_count,
            error: if completed {
                None
            } else {
                bounded_error(outcome.error)
            },
        });
    }

    let report = StandupCorpusRunReport {
        schema_version: "standup_corpus_run_v1".to_string(),
        started_at,
        completed_at: chrono::Utc::now().to_rfc3339(),
        provider,
        model,
        template_id: TEMPLATE_ID.to_string(),
        summary_language,
        requested: meeting_ids.len(),
        completed: items
            .iter()
            .filter(|item| item.status == "completed")
            .count(),
        skipped: items.iter().filter(|item| item.status == "skipped").count(),
        failed: items.iter().filter(|item| item.status == "failed").count(),
        items,
    };
    if let Some(path) = report_path {
        write_report(&path, &report)?;
    }
    let _ = app.emit("standup-corpus-run-complete", &report);
    Ok(report)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn start_standup_corpus_run<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_ids: Vec<String>,
    provider: String,
    model: String,
    summary_language: Option<String>,
    overwrite: Option<bool>,
    report_path: Option<String>,
) -> Result<StandupCorpusRunStarted, String> {
    let guard = CorpusRunGuard::acquire()?;
    let meeting_ids = normalized_ids(meeting_ids)?;
    let pool = state.db_manager.pool().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_standup_corpus_inner(
            app.clone(),
            pool,
            meeting_ids,
            provider,
            model,
            summary_language,
            overwrite.unwrap_or(false),
            report_path.map(PathBuf::from),
            guard,
        )
        .await
        {
            log::error!("Standup corpus run failed: {error}");
            let _ = app.emit("standup-corpus-run-error", error);
        }
    });
    Ok(StandupCorpusRunStarted {
        message: "Standup corpus run started".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_trimmed_deduplicated_and_bounded() {
        assert_eq!(
            normalized_ids(vec![" m1 ".into(), "m1".into(), "m2".into()]).unwrap(),
            vec!["m1", "m2"]
        );
        assert!(normalized_ids(vec![" ".into()]).is_err());
        assert!(
            normalized_ids((0..=MAX_CORPUS_MEETINGS).map(|i| format!("m{i}")).collect()).is_err()
        );
    }

    #[test]
    fn detects_only_versioned_standup_results() {
        assert!(has_standup_v2(Some(
            r#"{"standup_v2":{"schema_version":"standup_v2"}}"#
        )));
        assert!(!has_standup_v2(Some(r#"{"summary":"legacy"}"#)));
        assert!(!has_standup_v2(Some("invalid")));
    }

    #[test]
    fn report_errors_are_bounded_and_single_line() {
        let error = bounded_error(Some(format!("a\n{}", "b".repeat(800)))).unwrap();
        assert!(!error.contains('\n'));
        assert_eq!(error.chars().count(), 500);
    }

    #[tokio::test]
    async fn corpus_input_preserves_evidence_timestamps() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE meetings(id TEXT PRIMARY KEY, title TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE speakers(id INTEGER PRIMARY KEY, display_name TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE transcripts(id TEXT PRIMARY KEY, meeting_id TEXT, transcript TEXT, \
             timestamp TEXT, audio_start_time REAL, speaker TEXT, speaker_id INTEGER)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO meetings VALUES('m1', 'Standup')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO transcripts VALUES('t1', 'm1', 'Готово', 'fallback', 62.4, NULL, NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let (title, transcript) = meeting_input(&pool, "m1").await.unwrap();
        assert_eq!(title, "Standup");
        assert_eq!(transcript, "[01:02] Готово");
    }
}
