//! Deep Analytics multi-stage pipeline (background task).
//!
//! Orchestrates: deterministic dynamics -> 8 DeepSeek extraction stages -> synthesis ->
//! local score + HTML render. Speaker names are not this pipeline's business — they are
//! resolved unattended right after diarization (see
//! [`crate::pipeline::speaker_naming`]), so the report reads whatever names the meeting
//! already carries. Progress is streamed to
//! the frontend via Tauri events and mirrored into the `analytics_reports` row. The whole
//! report fails ONLY on: no transcript, privacy block, missing DeepSeek credentials, or
//! cancellation. Every other per-stage failure is soft — the stage is recorded as failed
//! and its section renders a "не удалось построить" placeholder while the pipeline
//! continues.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use serde_json::json;
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio_util::sync::CancellationToken;

use crate::database::repositories::analytics_report::{AnalyticsReportsRepository, TOTAL_STAGES};
use crate::database::repositories::meeting::MeetingsRepository;
use crate::database::repositories::speaker::{SpeakersRepository, TranscriptSpeakerSegment};
use crate::llm::providers::{deepseek, resolve_deepseek};
use crate::llm::{ensure_outbound_allowed, Purpose};
use crate::report::dynamics::{self, DynSegment, Dynamics};
use crate::report::prompts::{
    self, ClarifyAnswer, ClarifyQuestion, Classification, Commitments, DisagreementsConcepts,
    Insights, Numbers, Roles, ThreadsRisks, Topics,
};
use crate::report::render::{compute_score, render_report, RenderInput};

/// Stage id + short Russian progress label, in pipeline order. `stage_index` emitted to
/// the frontend is 1-based (position + 1); `total_stages` is [`TOTAL_STAGES`].
const STAGE_META: [(&str, &str); TOTAL_STAGES as usize] = [
    ("dynamics", "Анализ динамики разговора"),
    ("classify", "Классификация встречи"),
    ("topics", "Темы и повестка"),
    ("decisions", "Решения"),
    ("commitments", "Обязательства"),
    ("threads_risks", "Незакрытое и риски"),
    ("disagreements_concepts", "Разногласия и концепции"),
    ("numbers", "Числа встречи"),
    ("roles", "Роли на встрече"),
    ("insights", "Главное — синтез"),
    ("render", "Сборка отчёта"),
];

/// Sampling temperature for all structured extraction calls.
const STAGE_TEMPERATURE: f32 = 0.2;

// Cancellation tokens keyed by report id (mirrors the summary pipeline pattern).
static CANCELLATION_REGISTRY: Lazy<Arc<Mutex<HashMap<String, CancellationToken>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

fn register_token(report_id: &str) -> CancellationToken {
    let token = CancellationToken::new();
    if let Ok(mut reg) = CANCELLATION_REGISTRY.lock() {
        reg.insert(report_id.to_string(), token.clone());
    }
    token
}

fn cleanup_token(report_id: &str) {
    if let Ok(mut reg) = CANCELLATION_REGISTRY.lock() {
        reg.remove(report_id);
    }
}

/// Kept as a compatibility no-op for older frontends. New report runs never pause for
/// clarification and therefore have no answer receiver.
pub fn submit_answers(_report_id: &str, _answers: Vec<ClarifyAnswer>) {}

/// Signal cancellation for a running report. Returns true if a live token was found.
pub fn cancel_report(report_id: &str) -> bool {
    if let Ok(reg) = CANCELLATION_REGISTRY.lock() {
        if let Some(token) = reg.get(report_id) {
            token.cancel();
            return true;
        }
    }
    false
}

/// The DeepSeek model this run will use: `deepseek.model` setting, else the provider default.
pub async fn resolve_model(pool: &SqlitePool) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT value FROM app_settings_kv WHERE key = 'deepseek.model'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| deepseek::DEFAULT_MODEL.to_string())
}

/// Reconcile reports orphaned by an app restart. `waiting_input` is retained for rows
/// created by older builds that still had interactive clarification. Mark every active
/// row failed so the meeting can be regenerated. Idempotent; call once at startup.
pub async fn recover_interrupted_reports(pool: &SqlitePool) {
    match AnalyticsReportsRepository::fail_interrupted(
        pool,
        "Генерация отчёта была прервана перезапуском приложения. Запустите её заново.",
    )
    .await
    {
        Ok(0) => {}
        Ok(n) => log::info!("[report] reset {n} interrupted report(s) to failed after restart"),
        Err(e) => log::warn!("[report] could not reconcile interrupted reports: {e}"),
    }
}

// ---- speaker identity helpers (contract-defined fallback order) ----

fn speaker_label(seg: &TranscriptSpeakerSegment) -> String {
    if let Some(dn) = seg.display_name.as_deref().filter(|s| !s.trim().is_empty()) {
        return dn.to_string();
    }
    if let Some(sp) = seg.speaker.as_deref().filter(|s| !s.trim().is_empty()) {
        return sp.to_string();
    }
    if let Some(id) = seg.speaker_id {
        return format!("Спикер {id}");
    }
    "Спикер".to_string()
}

fn speaker_key(seg: &TranscriptSpeakerSegment) -> String {
    if let Some(id) = seg.speaker_id {
        return format!("id:{id}");
    }
    if let Some(sp) = seg.speaker.as_deref().filter(|s| !s.trim().is_empty()) {
        return format!("ch:{sp}");
    }
    format!("lbl:{}", speaker_label(seg))
}

/// Per-segment views the pipeline stages consume; also the input the automatic speaker
/// naming pass builds its id-labeled transcript from.
pub(crate) fn build_segment_views(
    segments: &[TranscriptSpeakerSegment],
) -> (Vec<DynSegment>, Vec<String>, Vec<String>) {
    let dyn_segments: Vec<DynSegment> = segments
        .iter()
        .map(|s| DynSegment {
            start: s.audio_start_time,
            text: s.text.clone(),
            speaker_key: speaker_key(s),
            speaker_label: speaker_label(s),
        })
        .collect();
    let seg_labels: Vec<String> = dyn_segments
        .iter()
        .map(|d| d.speaker_label.clone())
        .collect();
    let seg_texts: Vec<String> = dyn_segments.iter().map(|d| d.text.clone()).collect();
    (dyn_segments, seg_labels, seg_texts)
}

// ---- event / db helpers ----

async fn start_stage<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    report_id: &str,
    meeting_id: &str,
    pos: usize,
) {
    let (id, label) = STAGE_META[pos];
    let stage_index = (pos + 1) as i64;
    if let Err(e) = AnalyticsReportsRepository::update_stage(pool, report_id, id, stage_index).await
    {
        log::warn!("[report] failed to persist stage {id} for {report_id}: {e}");
    }
    let _ = app.emit(
        "analytics-report-progress",
        json!({
            "report_id": report_id,
            "meeting_id": meeting_id,
            "stage": id,
            "stage_index": stage_index,
            "total_stages": TOTAL_STAGES,
            "label": label,
        }),
    );
}

async fn fail<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    report_id: &str,
    meeting_id: &str,
    msg: &str,
) {
    log::warn!("[report] {report_id} failed: {msg}");
    if let Err(e) = AnalyticsReportsRepository::mark_failed(pool, report_id, msg).await {
        log::error!("[report] failed to mark {report_id} failed: {e}");
    }
    let _ = app.emit(
        "analytics-report-error",
        json!({ "report_id": report_id, "meeting_id": meeting_id, "error": msg }),
    );
    cleanup_token(report_id);
}

async fn finish_cancelled(pool: &SqlitePool, report_id: &str) {
    if let Err(e) = AnalyticsReportsRepository::mark_cancelled(pool, report_id).await {
        log::error!("[report] failed to mark {report_id} cancelled: {e}");
    }
    cleanup_token(report_id);
    log::info!("[report] {report_id} cancelled");
}

// ---- structured stage execution ----

pub(crate) fn strip_json_fences(s: &str) -> String {
    let t = s.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```JSON"))
        .or_else(|| t.strip_prefix("```"))
        .unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t).trim();
    // Isolate the outermost JSON object if the model added stray prose.
    match (t.find('{'), t.rfind('}')) {
        (Some(a), Some(b)) if b >= a => t[a..=b].to_string(),
        _ => t.to_string(),
    }
}

async fn attempt<T: DeserializeOwned>(
    client: &deepseek::DeepSeekClient,
    system: &str,
    user: &str,
) -> Result<T, String> {
    let raw = client
        .complete_json(system, user, STAGE_TEMPERATURE)
        .await?;
    let cleaned = strip_json_fences(&raw);
    serde_json::from_str::<T>(&cleaned).map_err(|e| format!("JSON parse failed: {e}"))
}

/// Run one LLM stage: call, parse; on failure retry ONCE with a stricter instruction; on
/// second failure return `None` (soft failure — caller records it and continues).
async fn run_stage<T: DeserializeOwned>(
    client: &deepseek::DeepSeekClient,
    system: &str,
    user: &str,
    stage: &str,
    failed: &mut Vec<String>,
) -> Option<T> {
    match attempt::<T>(client, system, user).await {
        Ok(v) => Some(v),
        Err(first) => {
            log::warn!("[report] stage {stage} first attempt failed: {first}; retrying");
            let retry_user = prompts::retry_suffix(user);
            match attempt::<T>(client, system, &retry_user).await {
                Ok(v) => Some(v),
                Err(second) => {
                    log::warn!("[report] stage {stage} failed after retry: {second}");
                    failed.push(stage.to_string());
                    None
                }
            }
        }
    }
}

fn sanitize_component(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Write the report HTML. Preferred location is the meeting's own folder (alongside its
/// audio/transcript); if `folder_path` is unset, unusable, or the write fails, fall back
/// to `{app_data_dir}/reports/{meeting_id}/`.
fn write_html<R: Runtime>(
    app: &AppHandle<R>,
    meeting_id: &str,
    folder_path: Option<&str>,
    html: &str,
) -> Result<String, String> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let filename = format!("deep_report_{ts}.html");

    // 1) Meeting's own folder.
    if let Some(fp) = folder_path.map(str::trim).filter(|s| !s.is_empty()) {
        let dir = PathBuf::from(fp);
        let usable = dir.is_dir() || std::fs::create_dir_all(&dir).is_ok();
        if usable {
            let path = dir.join(&filename);
            match std::fs::write(&path, html) {
                Ok(_) => return Ok(path.to_string_lossy().to_string()),
                Err(e) => log::warn!(
                    "[report] could not write into meeting folder {fp}: {e}; using app-data fallback"
                ),
            }
        } else {
            log::warn!("[report] meeting folder {fp} is unusable; using app-data fallback");
        }
    }

    // 2) Fallback under the app data directory.
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir unavailable: {e}"))?;
    let dir = base.join("reports").join(sanitize_component(meeting_id));
    std::fs::create_dir_all(&dir).map_err(|e| format!("create report dir failed: {e}"))?;
    let path = dir.join(&filename);
    std::fs::write(&path, html).map_err(|e| format!("write report file failed: {e}"))?;
    Ok(path.to_string_lossy().to_string())
}

fn russian_month(m: u32) -> &'static str {
    match m {
        1 => "января",
        2 => "февраля",
        3 => "марта",
        4 => "апреля",
        5 => "мая",
        6 => "июня",
        7 => "июля",
        8 => "августа",
        9 => "сентября",
        10 => "октября",
        11 => "ноября",
        _ => "декабря",
    }
}

/// Load a display title, Russian date string, and the meeting's storage folder (where the
/// report should be written). Falls back gracefully when the meeting row is missing.
async fn load_meeting(pool: &SqlitePool, meeting_id: &str) -> (String, String, Option<String>) {
    match MeetingsRepository::get_meeting_metadata(pool, meeting_id).await {
        Ok(Some(m)) => {
            use chrono::Datelike;
            let dt = m.created_at.0;
            let date_str = format!("{} {} {}", dt.day(), russian_month(dt.month()), dt.year());
            (m.title, date_str, m.folder_path)
        }
        _ => ("Встреча".to_string(), String::new(), None),
    }
}

/// Run the full pipeline. Intended to be spawned via `tauri::async_runtime::spawn`.
pub async fn run_report_pipeline<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    report_id: String,
    meeting_id: String,
    model: String,
) {
    let token = register_token(&report_id);
    let start = Instant::now();

    // ---- transcript (hard requirement) ----
    let segments = match SpeakersRepository::meeting_transcript_segments(&pool, &meeting_id).await {
        Ok(s) => s,
        Err(e) => {
            fail(
                &app,
                &pool,
                &report_id,
                &meeting_id,
                &format!("Не удалось прочитать транскрипт: {e}"),
            )
            .await;
            return;
        }
    };
    if segments.is_empty() {
        fail(
            &app,
            &pool,
            &report_id,
            &meeting_id,
            "В этой встрече нет транскрипта.",
        )
        .await;
        return;
    }

    let (meeting_title, date_str, folder_path) = load_meeting(&pool, &meeting_id).await;

    // ---- privacy gate + credentials (hard requirements, before first network call) ----
    if let Err(e) = ensure_outbound_allowed(&pool, Purpose::Extract).await {
        fail(&app, &pool, &report_id, &meeting_id, &e.to_string()).await;
        return;
    }
    let client = match resolve_deepseek(&pool).await {
        Ok(c) => c,
        Err(reason) => {
            fail(&app, &pool, &report_id, &meeting_id, &reason).await;
            return;
        }
    };

    let mut failed: Vec<String> = Vec::new();

    // ---- stage 1: dynamics (local) ----
    start_stage(&app, &pool, &report_id, &meeting_id, 0).await;
    let (dyn_segments, seg_labels, seg_texts) = build_segment_views(&segments);
    let timed = dynamics::timeline(&dyn_segments);
    let dyn_metrics = Dynamics::from_timed(&dyn_segments, &timed);

    let transcript =
        prompts::truncate_transcript(&prompts::format_transcript(&timed, &seg_labels, &seg_texts));

    // ---- stage 2: classify ----
    start_stage(&app, &pool, &report_id, &meeting_id, 1).await;
    let (sys, usr) = prompts::classify(&transcript);
    let classification: Option<Classification> =
        run_stage(&client, &sys, &usr, "classify", &mut failed).await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }
    let meeting_type = classification
        .as_ref()
        .map(|c| c.meeting_type.clone())
        .unwrap_or_default();

    // Clarification is intentionally skipped. Keep empty artifacts so old HTML and DB
    // contracts remain readable without ever pausing the new pipeline.
    let clarify_questions: Vec<ClarifyQuestion> = Vec::new();
    let clarify_answers: Vec<ClarifyAnswer> = Vec::new();
    let answers_block = String::new();

    // ---- stage 3: topics ----
    start_stage(&app, &pool, &report_id, &meeting_id, 2).await;
    let (sys, usr) = prompts::topics(&transcript, &meeting_type);
    let topics: Option<Topics> = run_stage(
        &client,
        &sys,
        &prompts::with_context(&usr, &answers_block),
        "topics",
        &mut failed,
    )
    .await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }

    // ---- stage 4: decisions ----
    start_stage(&app, &pool, &report_id, &meeting_id, 3).await;
    let (sys, usr) = prompts::decisions(&transcript);
    let decisions: Option<prompts::Decisions> = run_stage(
        &client,
        &sys,
        &prompts::with_context(&usr, &answers_block),
        "decisions",
        &mut failed,
    )
    .await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }

    // ---- stage 5: commitments ----
    start_stage(&app, &pool, &report_id, &meeting_id, 4).await;
    let (sys, usr) = prompts::commitments(&transcript);
    let commitments: Option<Commitments> = run_stage(
        &client,
        &sys,
        &prompts::with_context(&usr, &answers_block),
        "commitments",
        &mut failed,
    )
    .await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }

    // ---- stage 6: threads + risks ----
    start_stage(&app, &pool, &report_id, &meeting_id, 5).await;
    let (sys, usr) = prompts::threads_risks(&transcript);
    let threads_risks: Option<ThreadsRisks> = run_stage(
        &client,
        &sys,
        &prompts::with_context(&usr, &answers_block),
        "threads_risks",
        &mut failed,
    )
    .await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }

    // ---- stage 7: disagreements + concepts ----
    start_stage(&app, &pool, &report_id, &meeting_id, 6).await;
    let (sys, usr) = prompts::disagreements_concepts(&transcript);
    let disagreements_concepts: Option<DisagreementsConcepts> = run_stage(
        &client,
        &sys,
        &prompts::with_context(&usr, &answers_block),
        "disagreements_concepts",
        &mut failed,
    )
    .await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }

    // ---- stage 8: numbers ----
    start_stage(&app, &pool, &report_id, &meeting_id, 7).await;
    let (sys, usr) = prompts::numbers(&transcript);
    let numbers: Option<Numbers> = run_stage(
        &client,
        &sys,
        &prompts::with_context(&usr, &answers_block),
        "numbers",
        &mut failed,
    )
    .await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }

    // ---- stage 9: roles ----
    start_stage(&app, &pool, &report_id, &meeting_id, 8).await;
    let (sys, usr) = prompts::roles(&transcript);
    let roles: Option<Roles> = run_stage(
        &client,
        &sys,
        &prompts::with_context(&usr, &answers_block),
        "roles",
        &mut failed,
    )
    .await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }

    // ---- stage 10: insights (over artifacts + fast facts, NOT the transcript) ----
    start_stage(&app, &pool, &report_id, &meeting_id, 9).await;
    let artifacts_for_insights = json!({
        "classification": classification,
        "topics": topics,
        "decisions": decisions,
        "commitments": commitments,
        "threads_risks": threads_risks,
        "disagreements_concepts": disagreements_concepts,
        "numbers": numbers,
        "roles": roles,
    })
    .to_string();
    let fast = prompts::fast_facts(&dyn_metrics);
    let (sys, usr) = prompts::insights(&artifacts_for_insights, &fast);
    let insights: Option<Insights> = run_stage(
        &client,
        &sys,
        &prompts::with_context(&usr, &answers_block),
        "insights",
        &mut failed,
    )
    .await;
    if token.is_cancelled() {
        finish_cancelled(&pool, &report_id).await;
        return;
    }

    // ---- stage 11: score + render (local) ----
    start_stage(&app, &pool, &report_id, &meeting_id, 10).await;
    let empty_agenda = Vec::new();
    let agenda = topics.as_ref().map(|t| &t.agenda).unwrap_or(&empty_agenda);
    let empty_commitments = Vec::new();
    let commitments_vec = commitments
        .as_ref()
        .map(|c| &c.commitments)
        .unwrap_or(&empty_commitments);
    let open_len = threads_risks
        .as_ref()
        .map(|t| t.open_threads.len())
        .unwrap_or(0);
    let score = compute_score(agenda, commitments_vec, open_len);

    let generation_secs = start.elapsed().as_secs_f64();
    let html = render_report(&RenderInput {
        meeting_title: &meeting_title,
        date_str: &date_str,
        model: &model,
        generation_secs,
        total_stages: TOTAL_STAGES,
        dynamics: &dyn_metrics,
        timed: &timed,
        seg_labels: &seg_labels,
        seg_texts: &seg_texts,
        classification: classification.as_ref(),
        topics: topics.as_ref(),
        decisions: decisions.as_ref(),
        commitments: commitments.as_ref(),
        threads_risks: threads_risks.as_ref(),
        disagreements_concepts: disagreements_concepts.as_ref(),
        numbers: numbers.as_ref(),
        roles: roles.as_ref(),
        insights: insights.as_ref(),
        score: &score,
        clarify_questions: &clarify_questions,
        clarify_answers: &clarify_answers,
    });

    let html_path = match write_html(&app, &meeting_id, folder_path.as_deref(), &html) {
        Ok(p) => p,
        Err(e) => {
            fail(&app, &pool, &report_id, &meeting_id, &e).await;
            return;
        }
    };

    // `segment_count` lets a later reader tell whether the stored `seg` indices still line
    // up with the meeting's transcript (see `report::sections`).
    let artifacts_store = json!({
        "segment_count": seg_texts.len(),
        "dynamics": dyn_metrics,
        "score": score,
        "classification": classification,
        "clarify_questions": clarify_questions,
        "clarify_answers": clarify_answers,
        "topics": topics,
        "decisions": decisions,
        "commitments": commitments,
        "threads_risks": threads_risks,
        "disagreements_concepts": disagreements_concepts,
        "numbers": numbers,
        "roles": roles,
        "insights": insights,
        "failed_stages": failed,
    })
    .to_string();

    if let Err(e) =
        AnalyticsReportsRepository::mark_completed(&pool, &report_id, &html_path, &artifacts_store)
            .await
    {
        fail(
            &app,
            &pool,
            &report_id,
            &meeting_id,
            &format!("Не удалось сохранить отчёт: {e}"),
        )
        .await;
        return;
    }

    cleanup_token(&report_id);
    log::info!(
        "[report] {report_id} completed in {:.1}s ({} stage(s) degraded)",
        generation_secs,
        failed.len()
    );
    let _ = app.emit(
        "analytics-report-complete",
        json!({ "report_id": report_id, "meeting_id": meeting_id, "html_path": html_path }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_fences_handles_plain_and_fenced_and_noisy() {
        assert_eq!(strip_json_fences("{\"a\":1}"), "{\"a\":1}");
        assert_eq!(strip_json_fences("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(
            strip_json_fences("Вот ответ: {\"a\":1} — готово"),
            "{\"a\":1}"
        );
    }

    #[test]
    fn sanitize_component_strips_path_separators() {
        assert_eq!(sanitize_component("meeting-abc_123"), "meeting-abc_123");
        assert_eq!(sanitize_component("../../etc/passwd"), "______etc_passwd");
    }
}
