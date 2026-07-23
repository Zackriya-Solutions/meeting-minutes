//! Tauri commands for the Deep Analytics report feature. Contract is frozen — the
//! frontend depends on the exact command names, argument names, and payload field names.

use serde::Serialize;
use tauri::{AppHandle, Runtime, State};
use uuid::Uuid;

use crate::database::models::AnalyticsReportMeta;
use crate::database::repositories::analytics_report::AnalyticsReportsRepository;
use crate::report::pipeline;
use crate::report::prompts::ClarifyAnswer;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct GenerateAnalyticsReportResponse {
    pub report_id: String,
}

/// Start (or re-attach to) a Deep Analytics report for a meeting. If a report for this
/// meeting is already `queued`/`running`, its id is returned instead of starting a
/// duplicate. Otherwise a new row is inserted and the pipeline is spawned; returns
/// immediately with the new report id.
#[tauri::command]
pub async fn generate_analytics_report<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<GenerateAnalyticsReportResponse, String> {
    let pool = state.db_manager.pool().clone();

    if let Some(existing) =
        AnalyticsReportsRepository::active_report_id_for_meeting(&pool, &meeting_id)
            .await
            .map_err(|e| format!("Failed to check existing reports: {e}"))?
    {
        log::info!(
            "[report] reusing in-flight report {existing} for meeting {meeting_id}"
        );
        return Ok(GenerateAnalyticsReportResponse {
            report_id: existing,
        });
    }

    let report_id = Uuid::new_v4().to_string();
    let model = pipeline::resolve_model(&pool).await;

    AnalyticsReportsRepository::insert(&pool, &report_id, &meeting_id, &model)
        .await
        .map_err(|e| format!("Failed to create report: {e}"))?;

    log::info!(
        "[report] queued report {report_id} for meeting {meeting_id} (model {model})"
    );

    let app_clone = app.clone();
    let report_id_task = report_id.clone();
    let meeting_id_task = meeting_id.clone();
    tauri::async_runtime::spawn(async move {
        pipeline::run_report_pipeline(
            app_clone,
            pool,
            report_id_task,
            meeting_id_task,
            model,
        )
        .await;
    });

    Ok(GenerateAnalyticsReportResponse { report_id })
}

/// Latest report row for a meeting (any status), or `None` if never generated.
#[tauri::command]
pub async fn get_analytics_report(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<Option<AnalyticsReportMeta>, String> {
    let pool = state.db_manager.pool();
    AnalyticsReportsRepository::latest_for_meeting(pool, &meeting_id)
        .await
        .map_err(|e| format!("Failed to load report: {e}"))
}

/// Cancel a running report by id (cancels the token; row status -> `cancelled`).
#[tauri::command]
pub async fn cancel_analytics_report(
    state: State<'_, AppState>,
    report_id: String,
) -> Result<(), String> {
    let found = pipeline::cancel_report(&report_id);
    log::info!("[report] cancel requested for {report_id} (active token: {found})");
    let pool = state.db_manager.pool();
    AnalyticsReportsRepository::mark_cancelled(pool, &report_id)
        .await
        .map_err(|e| format!("Failed to cancel report: {e}"))
}

/// Submit the user's answers to a report parked in `waiting_input`. Idempotent: if no
/// pipeline is currently waiting for `report_id`, this returns Ok(()) silently. An empty
/// `answers` vec means "skip everything".
#[tauri::command]
pub async fn submit_analytics_answers(
    report_id: String,
    answers: Vec<ClarifyAnswer>,
) -> Result<(), String> {
    log::info!(
        "[report] answers submitted for {report_id} ({} answer(s))",
        answers.len()
    );
    pipeline::submit_answers(&report_id, answers);
    Ok(())
}

/// Open the OS file manager with `path` selected (macOS `open -R`, Windows
/// `explorer /select,`, Linux `xdg-open` on the parent directory). Validates existence.
#[tauri::command]
pub fn reveal_report_in_folder(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(format!("Файл отчёта не найден: {path}"));
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Не удалось открыть Finder: {e}"))?;
    }

    #[cfg(target_os = "windows")]
    {
        // Windows Explorer requires the selector and path as a SINGLE argument.
        std::process::Command::new("explorer")
            .arg(format!("/select,{path}"))
            .spawn()
            .map_err(|e| format!("Не удалось открыть Проводник: {e}"))?;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // No portable "select file" on Linux; open the containing directory.
        let dir = p.parent().unwrap_or(p);
        std::process::Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map_err(|e| format!("Не удалось открыть файловый менеджер: {e}"))?;
    }

    Ok(())
}
