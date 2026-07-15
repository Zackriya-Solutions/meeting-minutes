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
use futures_util::FutureExt;
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Runtime};

const MAX_CORPUS_MEETINGS: usize = 50;
const TEMPLATE_ID: &str = "daily_standup";
static CORPUS_RUN_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

fn corpus_mode_requested_from(raw: Option<&str>) -> bool {
    raw.is_some_and(|raw| {
        raw.split(',')
            .map(str::trim)
            .any(|meeting_id| !meeting_id.is_empty())
    })
}

pub(crate) fn corpus_mode_requested() -> bool {
    let raw = std::env::var("MEETILY_STANDUP_CORPUS_IDS").ok();
    corpus_mode_requested_from(raw.as_deref())
}

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
    pub state: String,
    pub started_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
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
         ORDER BY COALESCE(audio_start_time, 0.0), timestamp, id",
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
fn build_report(
    started_at: &str,
    provider: &str,
    model: &str,
    summary_language: &Option<String>,
    requested: usize,
    items: &[StandupCorpusRunItem],
    completed: bool,
) -> StandupCorpusRunReport {
    let now = chrono::Utc::now().to_rfc3339();
    StandupCorpusRunReport {
        schema_version: "standup_corpus_run_v2".to_string(),
        state: if completed { "completed" } else { "running" }.to_string(),
        started_at: started_at.to_string(),
        updated_at: now.clone(),
        completed_at: completed.then_some(now),
        provider: provider.to_string(),
        model: model.to_string(),
        template_id: TEMPLATE_ID.to_string(),
        summary_language: summary_language.clone(),
        requested,
        completed: items
            .iter()
            .filter(|item| item.status == "completed")
            .count(),
        skipped: items.iter().filter(|item| item.status == "skipped").count(),
        failed: items.iter().filter(|item| item.status == "failed").count(),
        items: items.to_vec(),
    }
}

fn failed_item(meeting_id: &str, title: String, error: impl ToString) -> StandupCorpusRunItem {
    StandupCorpusRunItem {
        meeting_id: meeting_id.to_string(),
        title,
        status: "failed".to_string(),
        processing_time_ms: 0,
        chunk_count: 0,
        extracted_record_count: 0,
        error: bounded_error(Some(error.to_string())),
    }
}

#[allow(clippy::too_many_arguments)]
async fn process_meeting<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    meeting_id: &str,
    provider: &str,
    model: &str,
    summary_language: &Option<String>,
    overwrite: bool,
    current: usize,
    total: usize,
) -> StandupCorpusRunItem {
    let (title, transcript) = match meeting_input(pool, meeting_id).await {
        Ok(value) => value,
        Err(error) => return failed_item(meeting_id, String::new(), error),
    };
    let _ = app.emit(
        "standup-corpus-run-progress",
        StandupCorpusRunProgress {
            current,
            total,
            meeting_id: meeting_id.to_string(),
            title: title.clone(),
            state: "processing".to_string(),
        },
    );

    let existing = match SummaryProcessesRepository::get_summary_data(pool, meeting_id).await {
        Ok(value) => value,
        Err(error) => return failed_item(meeting_id, title, error),
    };
    if !overwrite
        && existing
            .as_ref()
            .is_some_and(|row| row.status == "completed" && has_standup_v2(row.result.as_deref()))
    {
        let extracted_record_count =
            sqlx::query_scalar("SELECT COUNT(*) FROM standup_records WHERE meeting_id = ?")
                .bind(meeting_id)
                .fetch_one(pool)
                .await
                .unwrap_or(0);
        return StandupCorpusRunItem {
            meeting_id: meeting_id.to_string(),
            title,
            status: "skipped".to_string(),
            processing_time_ms: 0,
            chunk_count: existing
                .as_ref()
                .map(|row| row.chunk_count)
                .unwrap_or_default(),
            extracted_record_count,
            error: None,
        };
    }

    if let Err(error) = SummaryProcessesRepository::create_or_reset_process(pool, meeting_id).await
    {
        return failed_item(meeting_id, title, error);
    }
    if let Err(error) = TranscriptChunksRepository::save_transcript_data(
        pool,
        meeting_id,
        &transcript,
        provider,
        model,
        40_000,
        1_000,
    )
    .await
    {
        return failed_item(meeting_id, title, error);
    }
    SummaryService::process_transcript_background(
        app.clone(),
        pool.clone(),
        meeting_id.to_string(),
        transcript,
        provider.to_string(),
        model.to_string(),
        String::new(),
        TEMPLATE_ID.to_string(),
        summary_language.clone(),
    )
    .await;

    let outcome = match SummaryProcessesRepository::get_summary_data(pool, meeting_id).await {
        Ok(Some(value)) => value,
        Ok(None) => {
            return failed_item(
                meeting_id,
                title,
                "Summary process disappeared after generation",
            )
        }
        Err(error) => return failed_item(meeting_id, title, error),
    };
    let extracted_record_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM standup_records WHERE meeting_id = ?")
            .bind(meeting_id)
            .fetch_one(pool)
            .await
            .unwrap_or(0);
    let completed = outcome.status == "completed" && has_standup_v2(outcome.result.as_deref());
    StandupCorpusRunItem {
        meeting_id: meeting_id.to_string(),
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
    }
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

    if let Some(path) = report_path.as_deref() {
        write_report(
            path,
            &build_report(
                &started_at,
                &provider,
                &model,
                &summary_language,
                meeting_ids.len(),
                &items,
                false,
            ),
        )?;
    }

    for (index, meeting_id) in meeting_ids.iter().enumerate() {
        let item = match AssertUnwindSafe(process_meeting(
            &app,
            &pool,
            meeting_id,
            &provider,
            &model,
            &summary_language,
            overwrite,
            index + 1,
            meeting_ids.len(),
        ))
        .catch_unwind()
        .await
        {
            Ok(item) => item,
            Err(_) => failed_item(
                meeting_id,
                String::new(),
                "Standup pipeline panicked; inspect the local application log",
            ),
        };
        let _ = app.emit(
            "standup-corpus-run-progress",
            StandupCorpusRunProgress {
                current: index + 1,
                total: meeting_ids.len(),
                meeting_id: meeting_id.clone(),
                title: item.title.clone(),
                state: item.status.clone(),
            },
        );
        items.push(item);
        if let Some(path) = report_path.as_deref() {
            write_report(
                path,
                &build_report(
                    &started_at,
                    &provider,
                    &model,
                    &summary_language,
                    meeting_ids.len(),
                    &items,
                    false,
                ),
            )?;
        }
    }

    let report = build_report(
        &started_at,
        &provider,
        &model,
        &summary_language,
        meeting_ids.len(),
        &items,
        true,
    );
    if let Some(path) = report_path.as_deref() {
        write_report(path, &report)?;
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
    if !corpus_mode_requested() {
        return Err(
            "Standup corpus commands are available only in explicit isolated corpus mode"
                .to_string(),
        );
    }
    let meeting_ids = normalized_ids(meeting_ids)?;
    let provider = provider.trim().to_string();
    let model = model.trim().to_string();
    if provider.is_empty() || model.is_empty() {
        return Err("Provider and model are required".to_string());
    }
    let guard = CorpusRunGuard::acquire()?;
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
    fn corpus_mode_requires_at_least_one_nonempty_explicit_id() {
        assert!(!corpus_mode_requested_from(None));
        assert!(!corpus_mode_requested_from(Some(" ,  ,")));
        assert!(corpus_mode_requested_from(Some(" meeting-1 ")));
        assert!(corpus_mode_requested_from(Some(" , meeting-1,meeting-2")));
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

    #[test]
    fn report_checkpoints_have_stable_state_and_counts() {
        let items = vec![
            StandupCorpusRunItem {
                meeting_id: "m1".into(),
                title: "Completed".into(),
                status: "completed".into(),
                processing_time_ms: 10,
                chunk_count: 2,
                extracted_record_count: 3,
                error: None,
            },
            StandupCorpusRunItem {
                meeting_id: "m2".into(),
                title: "Skipped".into(),
                status: "skipped".into(),
                processing_time_ms: 0,
                chunk_count: 4,
                extracted_record_count: 5,
                error: None,
            },
            failed_item("m3", "Failed".into(), "provider error"),
        ];
        let language = Some("ru-RU".to_string());

        let running = build_report(
            "2026-07-15T00:00:00Z",
            "builtin-ai",
            "qwen3.5:4b",
            &language,
            4,
            &items,
            false,
        );
        assert_eq!(running.schema_version, "standup_corpus_run_v2");
        assert_eq!(running.state, "running");
        assert!(running.completed_at.is_none());
        assert_eq!(running.requested, 4);
        assert_eq!(
            (running.completed, running.skipped, running.failed),
            (1, 1, 1)
        );

        let completed = build_report(
            "2026-07-15T00:00:00Z",
            "builtin-ai",
            "qwen3.5:4b",
            &language,
            4,
            &items,
            true,
        );
        assert_eq!(completed.state, "completed");
        assert!(completed.completed_at.is_some());
        assert_eq!(completed.items.len(), 3);
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
        sqlx::query(
            "INSERT INTO transcripts VALUES('t0', 'm1', 'Без таймкода', '[00:00]', NULL, NULL, NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let (title, transcript) = meeting_input(&pool, "m1").await.unwrap();
        assert_eq!(title, "Standup");
        assert_eq!(transcript, "[00:00] Без таймкода\n[01:02] Готово");
    }
}
