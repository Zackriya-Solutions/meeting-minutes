//! Tauri commands for the Deep Analytics report feature. Contract is frozen — the
//! frontend depends on the exact command names, argument names, and payload field names.

use once_cell::sync::Lazy;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::audio::audio_processing::sanitize_filename;
use crate::database::models::AnalyticsReportMeta;
use crate::database::repositories::analytics_report::AnalyticsReportsRepository;
use crate::database::repositories::meeting::MeetingsRepository;
use crate::report::pipeline;
use crate::report::prompts::ClarifyAnswer;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct GenerateAnalyticsReportResponse {
    pub report_id: String,
}

// The active-report check and the insert below are not atomic in SQLite, so two
// near-simultaneous invocations (e.g. a double-click racing the first invoke's
// round-trip) could both pass the check and spawn duplicate pipelines. All writers
// live in this process, so serialising the check+insert here is sufficient.
static GENERATE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

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

    let _generate_guard = GENERATE_LOCK.lock().await;

    if let Some(existing) =
        AnalyticsReportsRepository::active_report_id_for_meeting(&pool, &meeting_id)
            .await
            .map_err(|e| format!("Failed to check existing reports: {e}"))?
    {
        log::info!("[report] reusing in-flight report {existing} for meeting {meeting_id}");
        return Ok(GenerateAnalyticsReportResponse {
            report_id: existing,
        });
    }

    let report_id = Uuid::new_v4().to_string();
    let model = pipeline::resolve_model(&pool).await;

    AnalyticsReportsRepository::insert(&pool, &report_id, &meeting_id, &model)
        .await
        .map_err(|e| format!("Failed to create report: {e}"))?;

    log::info!("[report] queued report {report_id} for meeting {meeting_id} (model {model})");

    let app_clone = app.clone();
    let report_id_task = report_id.clone();
    let meeting_id_task = meeting_id.clone();
    tauri::async_runtime::spawn(async move {
        pipeline::run_report_pipeline(app_clone, pool, report_id_task, meeting_id_task, model)
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

fn suggested_report_name(title: &str) -> String {
    let sanitized = sanitize_filename(title);
    let stem = if sanitized.is_empty() {
        "Memento"
    } else {
        sanitized.as_str()
    };
    format!("{stem} — аналитический отчёт.html")
}

fn ensure_html_extension(path: PathBuf) -> PathBuf {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("html"))
    {
        path
    } else {
        path.with_extension("html")
    }
}

/// Save the latest completed report for a meeting to a location chosen by the user.
/// The generated source remains in the meeting folder; exporting creates a separate copy.
#[tauri::command]
pub async fn download_analytics_report<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<Option<String>, String> {
    let pool = state.db_manager.pool();
    let report = AnalyticsReportsRepository::latest_for_meeting(pool, &meeting_id)
        .await
        .map_err(|e| format!("Не удалось загрузить аналитический отчёт: {e}"))?
        .ok_or_else(|| "Аналитический отчёт ещё не создан.".to_string())?;

    if report.status != "completed" {
        return Err("Аналитический отчёт ещё не готов.".to_string());
    }

    let source = report
        .html_path
        .map(PathBuf::from)
        .ok_or_else(|| "У готового отчёта отсутствует путь к HTML-файлу.".to_string())?;
    if !source.is_file() {
        return Err("HTML-файл аналитического отчёта не найден.".to_string());
    }

    let title = MeetingsRepository::get_meeting_metadata(pool, &meeting_id)
        .await
        .ok()
        .flatten()
        .map(|meeting| meeting.title)
        .unwrap_or_else(|| "Memento".to_string());

    let selected = app
        .dialog()
        .file()
        .add_filter("HTML", &["html"])
        .set_file_name(suggested_report_name(&title))
        .blocking_save_file();
    let Some(selected) = selected else {
        return Ok(None);
    };
    let destination = ensure_html_extension(
        selected
            .into_path()
            .map_err(|e| format!("Некорректный путь сохранения: {e}"))?,
    );

    if source != destination {
        let source_for_task = source.clone();
        let destination_for_task = destination.clone();
        tokio::task::spawn_blocking(move || std::fs::copy(source_for_task, destination_for_task))
            .await
            .map_err(|e| format!("Не удалось завершить сохранение отчёта: {e}"))?
            .map_err(|e| format!("Не удалось сохранить аналитический отчёт: {e}"))?;
    }

    Ok(Some(destination.to_string_lossy().into_owned()))
}

#[cfg(test)]
mod tests {
    use super::{ensure_html_extension, suggested_report_name};
    use std::path::PathBuf;

    #[test]
    fn report_download_name_is_safe_and_descriptive() {
        assert_eq!(
            suggested_report_name("Статус: продукт / команда"),
            "Статус_ продукт _ команда — аналитический отчёт.html"
        );
        assert_eq!(
            suggested_report_name("  "),
            "Memento — аналитический отчёт.html"
        );
    }

    #[test]
    fn report_download_always_uses_html_extension() {
        assert_eq!(
            ensure_html_extension(PathBuf::from("report")),
            PathBuf::from("report.html")
        );
        assert_eq!(
            ensure_html_extension(PathBuf::from("report.HTML")),
            PathBuf::from("report.HTML")
        );
    }
}
