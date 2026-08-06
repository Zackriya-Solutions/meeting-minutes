use crate::database::repositories::{
    meeting::MeetingsRepository, setting::SettingsRepository, summary::SummaryProcessesRepository,
    transcript_chunk::TranscriptChunksRepository,
};
use crate::state::AppState;
use crate::summary::language_detection::{detect_summary_language, SummaryLanguageDetection};
use crate::summary::metadata::{
    read_detected_summary_language_from_metadata, read_summary_language_from_metadata,
    write_detected_summary_language_to_metadata, write_summary_language_to_metadata,
};
use crate::summary::service::SummaryService;
use log::{error as log_error, info as log_info, warn as log_warn};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, Runtime};

const AUTOMATIC_SUMMARY_VERSION: &str = "automatic_summary_v2";

#[derive(Debug, Serialize, Deserialize)]
pub struct SummaryResponse {
    pub status: String,
    #[serde(rename = "meetingName")]
    pub meeting_name: Option<String>,
    pub meeting_id: String,
    pub start: Option<String>,
    pub end: Option<String>,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessTranscriptResponse {
    pub message: String,
    pub process_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SummaryLanguageStorage {
    Metadata,
    LocalFallback,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSummaryLanguagePreference {
    pub language: Option<String>,
    pub storage: SummaryLanguageStorage,
}

impl MeetingSummaryLanguagePreference {
    fn metadata(language: Option<String>) -> Self {
        Self {
            language,
            storage: SummaryLanguageStorage::Metadata,
        }
    }

    fn local_fallback() -> Self {
        Self {
            language: None,
            storage: SummaryLanguageStorage::LocalFallback,
        }
    }
}

enum MeetingFolderResolution {
    Folder(PathBuf),
    NoFolder,
}

async fn attach_summary_freshness(
    pool: &sqlx::SqlitePool,
    meeting_id: &str,
    summary: &mut serde_json::Value,
) {
    let stored_attribution = summary
        .pointer("/summary_generation/source/speaker_attribution_fingerprint")
        .and_then(serde_json::Value::as_str);
    let stored_transcript = summary
        .pointer("/summary_generation/source/transcript_fingerprint")
        .and_then(serde_json::Value::as_str);

    let current_segments = match crate::database::repositories::speaker::SpeakersRepository::meeting_transcript_segments(
        pool,
        meeting_id,
    )
    .await
    {
        Ok(segments) => segments,
        Err(error) => {
            log_warn!(
                "Failed to calculate summary freshness for {}: {}",
                meeting_id,
                error
            );
            return;
        }
    };

    let current_attribution =
        crate::summary::transcript_labeling::speaker_attribution_fingerprint(&current_segments);
    let (status, stale) = match (stored_attribution, current_attribution.as_deref()) {
        (Some(stored), Some(current)) => {
            if stored == current {
                ("current", false)
            } else {
                ("stale", true)
            }
        }
        (Some(_), None) => ("stale", true),
        (None, Some(_)) => {
            // Backward-compatible check for summaries generated before the dedicated speaker
            // snapshot existed. The source transcript already included rendered speaker labels,
            // so a mismatch catches a later rename/re-diarization (and conservatively also a
            // transcript edit).
            let current_labeled =
                crate::summary::transcript_labeling::assemble_labeled_transcript(&current_segments);
            let changed = current_labeled
                .as_deref()
                .map(crate::summary::service::stable_text_fingerprint)
                .zip(stored_transcript)
                .is_some_and(|(current, stored)| current != stored);
            if changed {
                ("legacy_source_changed", true)
            } else {
                ("unknown", false)
            }
        }
        (None, None) => ("not_applicable", false),
    };

    if let Some(object) = summary.as_object_mut() {
        object.insert(
            "summary_freshness".to_string(),
            serde_json::json!({
                "speaker_attribution_status": status,
                "speaker_attribution_stale": stale,
            }),
        );
    }
}

fn merge_manual_summary_with_generated_fields(
    existing_raw: Option<&str>,
    mut incoming: serde_json::Value,
) -> serde_json::Value {
    let Some(incoming_object) = incoming.as_object_mut() else {
        return incoming;
    };
    let Some(existing_object) = existing_raw
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|value| value.as_object().cloned())
    else {
        return incoming;
    };

    let markdown_changed = match (
        existing_object
            .get("markdown")
            .and_then(|value| value.as_str()),
        incoming_object
            .get("markdown")
            .and_then(|value| value.as_str()),
    ) {
        (Some(existing), Some(incoming)) => {
            markdown_semantic_text(existing) != markdown_semantic_text(incoming)
        }
        _ => existing_object.get("markdown") != incoming_object.get("markdown"),
    };
    for key in ["summary_generation", "standup_v2", "interview_v1"] {
        if !incoming_object.contains_key(key) {
            if let Some(value) = existing_object.get(key) {
                incoming_object.insert(key.to_string(), value.clone());
            }
        }
    }

    if markdown_changed && incoming_object.contains_key("standup_v2") {
        incoming_object.insert(
            "standup_v2_status".to_string(),
            serde_json::json!({
                "state": "markdown_edited",
                "structured_result_stale": true
            }),
        );
    } else if !incoming_object.contains_key("standup_v2_status") {
        if let Some(value) = existing_object.get("standup_v2_status") {
            incoming_object.insert("standup_v2_status".to_string(), value.clone());
        }
    }
    if markdown_changed && incoming_object.contains_key("interview_v1") {
        incoming_object.insert(
            "interview_v1_status".to_string(),
            serde_json::json!({
                "state": "markdown_edited",
                "structured_result_stale": true
            }),
        );
    } else if !incoming_object.contains_key("interview_v1_status") {
        if let Some(value) = existing_object.get("interview_v1_status") {
            incoming_object.insert("interview_v1_status".to_string(), value.clone());
        }
    }

    incoming
}

fn markdown_semantic_text(markdown: &str) -> String {
    markdown
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

async fn save_manual_summary_preserving_generated_fields(
    pool: &sqlx::SqlitePool,
    meeting_id: &str,
    incoming: serde_json::Value,
) -> Result<bool, String> {
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    let meeting_exists = sqlx::query("SELECT 1 FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?
        .is_some();
    if !meeting_exists {
        transaction
            .rollback()
            .await
            .map_err(|error| error.to_string())?;
        return Ok(false);
    }
    let existing_result: Option<String> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT result FROM summary_processes WHERE meeting_id = ?",
    )
    .bind(meeting_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?
    .flatten();
    let merged = merge_manual_summary_with_generated_fields(existing_result.as_deref(), incoming);
    let result_json = serde_json::to_string(&merged).map_err(|error| error.to_string())?;
    let now = chrono::Utc::now();
    sqlx::query("UPDATE summary_processes SET result = ?, updated_at = ? WHERE meeting_id = ?")
        .bind(result_json)
        .bind(now)
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query("UPDATE meetings SET updated_at = ? WHERE id = ?")
        .bind(now)
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    Ok(true)
}

/// Saves a meeting summary (Native SQLx implementation)
///
/// Expected format: { "markdown": "...", "summary_json": [...BlockNote blocks...] }
#[tauri::command]
pub async fn api_save_meeting_summary<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    summary: serde_json::Value,
    _auth_token: Option<String>,
) -> Result<serde_json::Value, String> {
    log_info!(
        "api_save_meeting_summary (native) called for meeting_id: {}",
        meeting_id
    );
    let pool = state.db_manager.pool();

    match save_manual_summary_preserving_generated_fields(pool, &meeting_id, summary).await {
        Ok(true) => {
            log_info!("Summary saved successfully for meeting_id: {}", meeting_id);

            // Finalize hook (PLAN.md Phase 0): a saved summary marks the meeting as
            // fully transcribed, so kick off the post-meeting knowledge-base pipeline
            // (chunk_embed -> diarize + extract). Fire-and-forget: never blocks the UI
            // and a failure here does not fail the save.
            match crate::jobs::enqueue_post_meeting_pipeline(pool, &meeting_id).await {
                Ok(job_id) => log_info!(
                    "Enqueued post-meeting pipeline job {} for meeting {}",
                    job_id,
                    meeting_id
                ),
                Err(e) => log_warn!(
                    "Failed to enqueue post-meeting pipeline for {}: {}",
                    meeting_id,
                    e
                ),
            }

            Ok(serde_json::json!({
                "message": "Meeting summary saved successfully"
            }))
        }
        Ok(false) => {
            log_warn!(
                "Meeting not found or invalid JSON for meeting_id: {}",
                meeting_id
            );
            Err("Meeting not found or can't convert the json".into())
        }
        Err(e) => {
            log_error!("Failed to save meeting summary for {}: {}", meeting_id, e);
            Err(e.to_string())
        }
    }
}

/// Gets the per-meeting summary language override from metadata.json.
#[tauri::command]
pub async fn api_get_meeting_summary_language<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<MeetingSummaryLanguagePreference, String> {
    log_info!(
        "api_get_meeting_summary_language called for meeting_id: {}",
        meeting_id
    );

    match resolve_meeting_folder(state.db_manager.pool(), &meeting_id).await? {
        MeetingFolderResolution::Folder(folder) => read_summary_language_from_metadata(&folder)
            .map(MeetingSummaryLanguagePreference::metadata)
            .map_err(|e| e.to_string()),
        MeetingFolderResolution::NoFolder => Ok(MeetingSummaryLanguagePreference::local_fallback()),
    }
}

/// Saves or clears the per-meeting summary language override in metadata.json.
#[tauri::command]
pub async fn api_save_meeting_summary_language<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    summary_language: Option<String>,
) -> Result<MeetingSummaryLanguagePreference, String> {
    log_info!(
        "api_save_meeting_summary_language called for meeting_id: {}, language: {:?}",
        meeting_id,
        summary_language
    );

    match resolve_meeting_folder(state.db_manager.pool(), &meeting_id).await? {
        MeetingFolderResolution::Folder(folder) => {
            write_summary_language_to_metadata(&folder, summary_language.as_deref())
                .map_err(|e| e.to_string())?;
            read_summary_language_from_metadata(&folder)
                .map(MeetingSummaryLanguagePreference::metadata)
                .map_err(|e| e.to_string())
        }
        MeetingFolderResolution::NoFolder => Ok(MeetingSummaryLanguagePreference::local_fallback()),
    }
}

/// Gets the cached Auto-detected summary language from metadata.json.
#[tauri::command]
pub async fn api_get_meeting_detected_summary_language<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<MeetingSummaryLanguagePreference, String> {
    log_info!(
        "api_get_meeting_detected_summary_language called for meeting_id: {}",
        meeting_id
    );

    match resolve_meeting_folder(state.db_manager.pool(), &meeting_id).await? {
        MeetingFolderResolution::Folder(folder) => {
            read_detected_summary_language_from_metadata(&folder)
                .map(MeetingSummaryLanguagePreference::metadata)
                .map_err(|e| e.to_string())
        }
        MeetingFolderResolution::NoFolder => Ok(MeetingSummaryLanguagePreference::local_fallback()),
    }
}

/// Saves or clears the cached Auto-detected summary language in metadata.json.
#[tauri::command]
pub async fn api_save_meeting_detected_summary_language<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    detected_summary_language: Option<String>,
) -> Result<MeetingSummaryLanguagePreference, String> {
    log_info!(
        "api_save_meeting_detected_summary_language called for meeting_id: {}, language: {:?}",
        meeting_id,
        detected_summary_language
    );

    match resolve_meeting_folder(state.db_manager.pool(), &meeting_id).await? {
        MeetingFolderResolution::Folder(folder) => {
            write_detected_summary_language_to_metadata(
                &folder,
                detected_summary_language.as_deref(),
            )
            .map_err(|e| e.to_string())?;
            read_detected_summary_language_from_metadata(&folder)
                .map(MeetingSummaryLanguagePreference::metadata)
                .map_err(|e| e.to_string())
        }
        MeetingFolderResolution::NoFolder => Ok(MeetingSummaryLanguagePreference::local_fallback()),
    }
}

/// Detects the dominant supported summary language from transcript segments.
#[tauri::command]
pub async fn api_detect_transcript_summary_language(
    transcript_texts: Vec<String>,
) -> Result<SummaryLanguageDetection, String> {
    Ok(detect_summary_language(&transcript_texts))
}

async fn resolve_meeting_folder(
    pool: &sqlx::SqlitePool,
    meeting_id: &str,
) -> Result<MeetingFolderResolution, String> {
    let meeting = MeetingsRepository::get_meeting_metadata(pool, meeting_id)
        .await
        .map_err(|e| format!("Failed to load meeting metadata: {}", e))?
        .ok_or_else(|| format!("Meeting not found: {}", meeting_id))?;

    let Some(folder_path) = meeting.folder_path.filter(|p| !p.trim().is_empty()) else {
        return Ok(MeetingFolderResolution::NoFolder);
    };

    Ok(MeetingFolderResolution::Folder(PathBuf::from(folder_path)))
}

/// Gets summary status and data (Native SQLx implementation)
///
/// Returns summary status (pending/processing/completed/failed) and parsed result data
#[tauri::command]
pub async fn api_get_summary<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    _auth_token: Option<String>,
) -> Result<SummaryResponse, String> {
    log_info!(
        "api_get_summary (native) called for meeting_id: {}",
        meeting_id
    );
    let pool = state.db_manager.pool();

    match SummaryProcessesRepository::get_summary_data_for_meeting(pool, &meeting_id).await {
        Ok(Some(process)) => {
            let status = process.status.to_lowercase();
            let error = process.error;

            // Parse result data if it exists (regardless of status)
            // This allows displaying restored summaries after cancellation or failure
            let durable_result = process.result.or_else(|| {
                if process.result_backup.is_some() {
                    log_warn!(
                        "Summary {} has no primary result; serving its recovery backup",
                        meeting_id
                    );
                }
                process.result_backup
            });
            let data = if let Some(result_str) = durable_result {
                match serde_json::from_str::<serde_json::Value>(&result_str) {
                    Ok(mut parsed) => {
                        crate::summary::processor::localize_summary_result_for_display(&mut parsed);
                        attach_summary_freshness(pool, &meeting_id, &mut parsed).await;
                        Some(parsed)
                    }
                    Err(e) => {
                        log_error!("Failed to parse summary result JSON: {}", e);
                        None
                    }
                }
            } else {
                None
            };

            // Fetch meeting title from database
            let meeting_name = match MeetingsRepository::get_meeting(pool, &meeting_id).await {
                Ok(Some(meeting_details)) => {
                    log_info!("Fetched meeting title: {}", &meeting_details.title);
                    Some(meeting_details.title)
                }
                Ok(None) => {
                    log_warn!("Meeting not found for meeting_id: {}", meeting_id);
                    None
                }
                Err(e) => {
                    log_error!("Failed to fetch meeting title: {}", e);
                    None
                }
            };

            let response = SummaryResponse {
                status: status.clone(),
                meeting_name,
                meeting_id: meeting_id.clone(),
                start: process.start_time.map(|t| t.to_rfc3339()),
                end: process.end_time.map(|t| t.to_rfc3339()),
                data,
                error,
            };

            log_info!(
                "Summary status for {}: {}, has_data: {}, meeting_name: {:?}",
                meeting_id,
                status,
                response.data.is_some(),
                response.meeting_name
            );
            Ok(response)
        }
        Ok(None) => {
            log_info!("No summary process found for meeting_id: {}", meeting_id);

            // Still fetch meeting title for idle state
            let meeting_name = match MeetingsRepository::get_meeting(pool, &meeting_id).await {
                Ok(Some(meeting_details)) => Some(meeting_details.title),
                _ => None,
            };

            Ok(SummaryResponse {
                status: "idle".to_string(),
                meeting_name,
                meeting_id,
                start: None,
                end: None,
                data: None,
                error: None,
            })
        }
        Err(e) => {
            log_error!("Error retrieving summary for {}: {}", meeting_id, e);
            Err(format!("Failed to retrieve summary: {}", e))
        }
    }
}

/// Processes transcript and generates summary (Native SQLx implementation)
///
/// Spawns a background task and returns immediately with process_id
async fn start_summary_process<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    text: String,
    model: String,
    model_name: String,
    meeting_id: String,
    chunk_size: Option<i32>,
    overlap: Option<i32>,
    custom_prompt: Option<String>,
    template_id: Option<String>,
    summary_language: Option<String>,
) -> Result<ProcessTranscriptResponse, String> {
    let m_id = meeting_id;
    log_info!(
        "Starting summary process for meeting_id: {}, model: {}",
        &m_id,
        &model
    );

    // Rebuild the summary input with speaker labels when this meeting's stored transcripts
    // carry speaker info (a diarized `speaker_id` or a 'mic'/'system' channel tag).
    let text =
        crate::summary::transcript_labeling::build_speaker_labeled_transcript(&pool, &m_id, text)
            .await;

    let final_prompt = custom_prompt.unwrap_or_default();
    let final_template_id = template_id.unwrap_or_else(|| "daily_standup".to_string());
    let summary_language = summary_language.and_then(|language| {
        let language = language.trim();
        (!language.is_empty()).then(|| language.to_string())
    });

    SummaryProcessesRepository::create_or_reset_process(&pool, &m_id)
        .await
        .map_err(|error| format!("Failed to initialize process: {error}"))?;

    let chunk_size = chunk_size.unwrap_or(40000);
    let overlap = overlap.unwrap_or(1000);
    TranscriptChunksRepository::save_transcript_data(
        &pool,
        &m_id,
        &text,
        &model,
        &model_name,
        chunk_size,
        overlap,
    )
    .await
    .map_err(|error| format!("Failed to save transcript data: {error}"))?;

    let meeting_id_clone = m_id.clone();
    tauri::async_runtime::spawn(async move {
        SummaryService::process_transcript_background(
            app,
            pool,
            meeting_id_clone,
            text,
            model,
            model_name,
            final_prompt,
            final_template_id,
            summary_language,
        )
        .await;
    });

    Ok(ProcessTranscriptResponse {
        message: "Summary generation started".to_string(),
        process_id: m_id,
    })
}

/// Start the configured summary model for a durable meeting, unless that meeting already
/// has a result or an active generation. The version marker makes startup recovery retry an
/// old failure once without hammering a broken/paid provider on every application launch.
pub async fn start_automatic_summary_for_meeting<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    meeting_id: &str,
) -> Result<bool, String> {
    let existing: Option<(String, Option<String>, bool)> = sqlx::query_as(
        "SELECT status, metadata, result IS NOT NULL FROM summary_processes WHERE meeting_id=?",
    )
    .bind(meeting_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| error.to_string())?;

    let mut metadata = serde_json::Map::new();
    if let Some((status, raw_metadata, has_result)) = existing {
        let normalized = status.to_ascii_lowercase();
        if has_result || matches!(normalized.as_str(), "pending" | "processing" | "completed") {
            return Ok(false);
        }
        if let Some(raw_metadata) = raw_metadata {
            metadata = serde_json::from_str::<serde_json::Value>(&raw_metadata)
                .ok()
                .and_then(|value| value.as_object().cloned())
                .unwrap_or_default();
        }
        if metadata
            .get("automatic_summary_version")
            .and_then(serde_json::Value::as_str)
            == Some(AUTOMATIC_SUMMARY_VERSION)
        {
            return Ok(false);
        }
    }

    let transcript_rows: Vec<String> = sqlx::query_scalar(
        "SELECT transcript FROM transcripts \
         WHERE meeting_id=? AND length(trim(transcript)) > 0 \
         ORDER BY CASE WHEN audio_start_time IS NULL THEN 1 ELSE 0 END, \
                  audio_start_time, timestamp, id",
    )
    .bind(meeting_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| error.to_string())?;
    let text = transcript_rows.join("\n");
    if text.trim().is_empty() {
        return Ok(false);
    }

    let config = SettingsRepository::get_model_config(&pool)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "No summary model configured".to_string())?;
    let template_id = MeetingsRepository::get_summary_template_id(&pool, meeting_id)
        .await
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| "standard_meeting".to_string());
    let summary_language = match resolve_meeting_folder(&pool, meeting_id).await {
        Ok(MeetingFolderResolution::Folder(folder)) => {
            read_summary_language_from_metadata(&folder).ok().flatten()
        }
        Ok(MeetingFolderResolution::NoFolder) | Err(_) => None,
    };

    start_summary_process(
        app,
        pool.clone(),
        text,
        config.provider,
        config.model,
        meeting_id.to_string(),
        Some(40000),
        Some(1000),
        None,
        Some(template_id),
        summary_language,
    )
    .await?;

    metadata.insert(
        "automatic_summary_version".to_string(),
        serde_json::Value::String(AUTOMATIC_SUMMARY_VERSION.to_string()),
    );
    metadata.insert(
        "automatic_summary_source".to_string(),
        serde_json::Value::String("meeting_saved".to_string()),
    );
    sqlx::query("UPDATE summary_processes SET metadata=? WHERE meeting_id=?")
        .bind(serde_json::Value::Object(metadata).to_string())
        .bind(meeting_id)
        .execute(&pool)
        .await
        .map_err(|error| error.to_string())?;

    Ok(true)
}

/// Fire-and-forget entry point used by every meeting creation path.
pub fn spawn_automatic_summary_for_meeting<R: Runtime>(app: AppHandle<R>, meeting_id: String) {
    tauri::async_runtime::spawn(async move {
        // The renderer persists the per-meeting language preference immediately after save.
        // Give that tiny write a chance to land; explicit UI generation wins the race and the
        // status guard below makes this fallback a no-op.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let Some(state) = app.try_state::<AppState>() else {
            log_warn!("Could not start automatic summary for {meeting_id}: app state unavailable");
            return;
        };
        let pool = state.db_manager.pool().clone();
        drop(state);
        match start_automatic_summary_for_meeting(app, pool, &meeting_id).await {
            Ok(true) => log_info!("Automatic summary started for meeting {meeting_id}"),
            Ok(false) => {}
            Err(error) => {
                log_warn!("Could not start automatic summary for meeting {meeting_id}: {error}")
            }
        }
    });
}

pub const INTERRUPTED_SUMMARY_ERROR: &str =
    "Summary generation was interrupted before it finished (the app closed while it was running).";

/// Clears generations that were still marked as running when the app last exited.
///
/// Must run before [`backfill_missing_automatic_summaries`]: while such a row claims to be
/// pending, the backfill treats the meeting as already in progress and the drawer polls that
/// status forever. `launched_at` must be captured at startup so that a generation queued by
/// the current session is never mistaken for a leftover. Returns the recovered meeting ids.
pub async fn recover_interrupted_summaries(
    pool: &SqlitePool,
    launched_at: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<String>, String> {
    SummaryProcessesRepository::fail_interrupted_processes(
        pool,
        INTERRUPTED_SUMMARY_ERROR,
        launched_at,
    )
    .await
    .map_err(|error| error.to_string())
}

/// Catch up meetings created by older versions or interrupted before generation started.
pub async fn backfill_missing_automatic_summaries<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
) -> Result<usize, String> {
    let meeting_ids: Vec<String> = sqlx::query_scalar(
        "SELECT m.id FROM meetings m \
         WHERE EXISTS (SELECT 1 FROM transcripts t \
                       WHERE t.meeting_id=m.id AND length(trim(t.transcript)) > 0) \
         ORDER BY COALESCE(m.occurred_at, m.created_at), m.id",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| error.to_string())?;

    let mut started = 0usize;
    for meeting_id in meeting_ids {
        if start_automatic_summary_for_meeting(app.clone(), pool.clone(), &meeting_id).await? {
            started += 1;
        }
    }
    Ok(started)
}

#[tauri::command]
pub async fn api_process_transcript<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    text: String,
    model: String,
    model_name: String,
    meeting_id: Option<String>,
    _chunk_size: Option<i32>,
    _overlap: Option<i32>,
    custom_prompt: Option<String>,
    template_id: Option<String>,
    summary_language: Option<String>,
    _auth_token: Option<String>,
) -> Result<ProcessTranscriptResponse, String> {
    use uuid::Uuid;

    let m_id = meeting_id.unwrap_or_else(|| format!("meeting-{}", Uuid::new_v4()));
    log_info!(
        "api_process_transcript (native) called for meeting_id: {}, model: {}",
        &m_id,
        &model
    );

    start_summary_process(
        app,
        state.db_manager.pool().clone(),
        text,
        model,
        model_name,
        m_id,
        _chunk_size,
        _overlap,
        custom_prompt,
        template_id,
        summary_language,
    )
    .await
}

/// Cancels an ongoing summary generation process
///
/// This command triggers the cancellation token for the specified meeting,
/// stopping the summary generation gracefully.
#[tauri::command]
pub async fn api_cancel_summary<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<serde_json::Value, String> {
    log_info!("api_cancel_summary called for meeting_id: {}", meeting_id);

    // Trigger cancellation via the service
    let cancelled = SummaryService::cancel_summary(&meeting_id);

    if cancelled {
        // Update database status to cancelled
        let pool = state.db_manager.pool();
        if let Err(e) =
            SummaryProcessesRepository::update_process_cancelled(pool, &meeting_id).await
        {
            log_error!(
                "Failed to update DB status to cancelled for {}: {}",
                meeting_id,
                e
            );
            return Err(format!("Failed to update cancellation status: {}", e));
        }

        log_info!(
            "Successfully cancelled summary generation for meeting_id: {}",
            meeting_id
        );
        Ok(serde_json::json!({
            "message": "Summary generation cancelled successfully",
            "meeting_id": meeting_id,
        }))
    } else {
        log_warn!(
            "No active summary generation found for meeting_id: {}",
            meeting_id
        );
        Ok(serde_json::json!({
            "message": "No active summary generation to cancel",
            "meeting_id": meeting_id,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_markdown_save_preserves_generation_and_marks_standup_stale() {
        let existing = serde_json::json!({
            "markdown": "old",
            "summary_generation": {"pipeline_version": 2},
            "standup_v2": {"schema_version": "standup_v2", "action_items": []}
        });
        let incoming = serde_json::json!({"markdown": "edited", "summary_json": []});
        let merged = merge_manual_summary_with_generated_fields(
            Some(&serde_json::to_string(&existing).unwrap()),
            incoming,
        );
        assert_eq!(merged["summary_generation"]["pipeline_version"], 2);
        assert_eq!(merged["standup_v2"]["schema_version"], "standup_v2");
        assert_eq!(merged["standup_v2_status"]["structured_result_stale"], true);
    }

    #[test]
    fn unchanged_markdown_does_not_mark_standup_stale() {
        let existing = serde_json::json!({
            "markdown": "same",
            "standup_v2": {"schema_version": "standup_v2"}
        });
        let merged = merge_manual_summary_with_generated_fields(
            Some(&serde_json::to_string(&existing).unwrap()),
            serde_json::json!({"markdown": "same"}),
        );
        assert!(merged.get("standup_v2_status").is_none());
    }

    #[test]
    fn formatting_only_markdown_round_trip_does_not_mark_standup_stale() {
        let existing = serde_json::json!({
            "markdown": "## Action items\n\n- **Deploy** `v2`",
            "standup_v2": {"schema_version": "standup_v2"}
        });
        let merged = merge_manual_summary_with_generated_fields(
            Some(&serde_json::to_string(&existing).unwrap()),
            serde_json::json!({"markdown": "# Action items\r\n\r\n* Deploy v2"}),
        );
        assert!(merged.get("standup_v2_status").is_none());
    }
}
