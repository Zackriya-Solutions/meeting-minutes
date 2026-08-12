use crate::database::repositories::meeting::MeetingsRepository;
use crate::state::AppState;
use log::{error as log_error, info as log_info};
use serde::Serialize;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

/// Result of exporting a meeting to markdown
#[derive(Debug, Serialize)]
pub struct ExportResult {
    pub file_path: String,
    pub success: bool,
    pub bytes_written: u64,
}

/// Fetches meeting transcripts + summary from the DB and returns a formatted
/// markdown string.  The caller can either present this string to the user or
/// pass it to [`api_export_meeting_markdown`] for file-writing.
///
/// # Markdown format
///
/// ```text
/// # {meeting_title}
///
/// **Date:** {created_at}
///
/// ---
///
/// ## Summary
///
/// {summary_markdown}
///
/// ---
///
/// ## Transcript
///
/// ### Segment 1 — {timestamp}
///
/// {transcript_text}
///
/// …
/// ```
async fn build_meeting_markdown(
    pool: &sqlx::SqlitePool,
    meeting_id: &str,
) -> Result<String, String> {
    // 1. Fetch meeting details
    let meeting = MeetingsRepository::get_meeting_metadata(pool, meeting_id)
        .await
        .map_err(|e| format!("Failed to fetch meeting: {}", e))?
        .ok_or_else(|| format!("Meeting {} not found", meeting_id))?;

    // 2. Fetch transcripts (ordered by audio_start_time)
    let (transcripts, _) =
        MeetingsRepository::get_meeting_transcripts_paginated(pool, meeting_id, 10000, 0)
            .await
            .map_err(|e| format!("Failed to fetch transcripts: {}", e))?;

    // 3. Fetch summary markdown from the `result` JSON column
    let summary_markdown: Option<String> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT result FROM summary_processes WHERE meeting_id = ? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to fetch summary: {}", e))?
    .flatten()
    .and_then(|result_str| {
        // The result column stores JSON: { "markdown": "...", "english_cache": {...} }
        let parsed: serde_json::Value = serde_json::from_str(&result_str).ok()?;
        parsed
            .get("markdown")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    });

    // 4. Build markdown content
    let mut md = String::new();

    // Header
    md.push_str(&format!("# {}\n\n", meeting.title));

    // Date
    md.push_str(&format!(
        "**Date:** {}\n\n",
        meeting.created_at.format("%Y-%m-%d %H:%M")
    ));

    // Separator
    md.push_str("---\n\n");

    // Summary section
    md.push_str("## Summary\n\n");
    if let Some(ref summary) = summary_markdown {
        md.push_str(summary);
        md.push('\n');
    } else {
        md.push_str("_No summary available._\n");
    }

    // Separator
    md.push_str("\n---\n\n");

    // Transcript section
    md.push_str("## Transcript\n\n");
    if transcripts.is_empty() {
        md.push_str("_No transcript data available._\n");
    } else {
        for (idx, segment) in transcripts.iter().enumerate() {
            let segment_num = idx + 1;

            // Format timestamp
            let timestamp_str = segment
                .audio_start_time
                .map(|t| {
                    let total_secs = t as u64;
                    format!("{:02}:{:02}", total_secs / 60, total_secs % 60)
                })
                .unwrap_or_else(|| segment.timestamp.clone());

            md.push_str(&format!(
                "### Segment {} — {}\n\n",
                segment_num, timestamp_str
            ));
            md.push_str(&segment.transcript);
            md.push_str("\n\n");
        }
    }

    Ok(md)
}

/// Gets the meeting markdown as a string without saving to file.
///
/// Returns the formatted markdown that can be used for clipboard copy,
/// preview, or other programmatic access.
#[tauri::command]
pub async fn api_get_meeting_markdown(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<String, String> {
    log_info!(
        "api_get_meeting_markdown called for meeting_id: {}",
        meeting_id
    );

    let pool = state.db_manager.pool();
    build_meeting_markdown(pool, &meeting_id).await
}

/// Exports a meeting to a markdown file via a save dialog.
///
/// Internally calls `build_meeting_markdown` to generate the content,
/// then shows a native save dialog and writes the `.md` file.
#[tauri::command]
pub async fn api_export_meeting_markdown<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<ExportResult, String> {
    log_info!(
        "api_export_meeting_markdown called for meeting_id: {}",
        meeting_id
    );

    let pool = state.db_manager.pool();

    // 1. Build markdown content
    let md = build_meeting_markdown(pool, &meeting_id).await?;

    // 2. Fetch meeting title for the default filename
    let meeting = MeetingsRepository::get_meeting(pool, &meeting_id)
        .await
        .map_err(|e| format!("Failed to fetch meeting: {}", e))?
        .ok_or_else(|| format!("Meeting {} not found", meeting_id))?;

    // Sanitize title for use as filename
    let safe_title: String = meeting
        .title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .replace(' ', "_");

    // 3. Show native save dialog
    let file_path = app
        .dialog()
        .file()
        .add_filter("Markdown", &["md"])
        .set_file_name(&format!("{}.md", safe_title))
        .blocking_save_file();

    let file_path = match file_path {
        Some(path) => path.to_string(),
        None => {
            log_info!("User cancelled export dialog");
            return Err("User cancelled export".to_string());
        }
    };

    // 4. Write file
    std::fs::write(&file_path, &md).map_err(|e| {
        let msg = format!("Failed to write file: {}", e);
        log_error!("{}", msg);
        msg
    })?;

    let bytes_written = md.len() as u64;
    log_info!(
        "Exported meeting {} to {} ({} bytes)",
        meeting_id,
        file_path,
        bytes_written
    );

    Ok(ExportResult {
        file_path,
        success: true,
        bytes_written,
    })
}
