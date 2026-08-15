use crate::database::models::{DateTimeUtc, Transcript};
use crate::database::repositories::meeting::MeetingsRepository;
use crate::state::AppState;
use chrono::Utc;
use log::{error as log_error, info as log_info};
use serde::Serialize;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

/// Result of exporting a meeting to a file
#[derive(Debug, Serialize)]
pub struct ExportResult {
    pub file_path: String,
    pub success: bool,
    pub bytes_written: u64,
}

/// Internal struct bundling all data required to render an export
/// (markdown or HTML) for a single meeting.
struct MeetingExportData {
    title: String,
    created_at: DateTimeUtc,
    transcripts: Vec<Transcript>,
    summary_markdown: Option<String>,
}

/// Fetches meeting metadata, transcripts, and the latest summary markdown
/// from the DB. Shared by all export renderers to avoid duplicated queries.
async fn fetch_export_data(
    pool: &sqlx::SqlitePool,
    meeting_id: &str,
) -> Result<MeetingExportData, String> {
    // 1. Meeting details
    let meeting = MeetingsRepository::get_meeting_metadata(pool, meeting_id)
        .await
        .map_err(|e| format!("Failed to fetch meeting: {}", e))?
        .ok_or_else(|| format!("Meeting {} not found", meeting_id))?;

    // 2. Transcripts (ordered by audio_start_time)
    let (transcripts, _) =
        MeetingsRepository::get_meeting_transcripts_paginated(pool, meeting_id, 10000, 0)
            .await
            .map_err(|e| format!("Failed to fetch transcripts: {}", e))?;

    // 3. Summary markdown from the `result` JSON column
    let summary_markdown: Option<String> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT result FROM summary_processes WHERE meeting_id = ? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to fetch summary: {}", e))?
    .flatten()
    .and_then(|result_str| {
        let parsed: serde_json::Value = serde_json::from_str(&result_str).ok()?;
        parsed
            .get("markdown")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    });

    Ok(MeetingExportData {
        title: meeting.title,
        created_at: meeting.created_at,
        transcripts,
        summary_markdown,
    })
}

/// Formats an audio-start offset (seconds) into `MM:SS` for transcript labels.
fn format_segment_timestamp(segment: &Transcript) -> String {
    segment
        .audio_start_time
        .map(|t| {
            let total_secs = t as u64;
            format!("{:02}:{:02}", total_secs / 60, total_secs % 60)
        })
        .unwrap_or_else(|| segment.timestamp.clone())
}

/// Builds the Markdown representation of a meeting (full document).
/// For section-filtered output use `build_meeting_markdown_ex`.
async fn build_meeting_markdown(
    pool: &sqlx::SqlitePool,
    meeting_id: &str,
) -> Result<String, String> {
    build_meeting_markdown_ex(pool, meeting_id, ExportSection::Full).await
}

/// Builds a self-contained HTML document for a meeting (full document).
/// For section-filtered output use `build_meeting_html_ex`.
async fn build_meeting_html(
    pool: &sqlx::SqlitePool,
    meeting_id: &str,
) -> Result<String, String> {
    build_meeting_html_ex(pool, meeting_id, ExportSection::Full).await
}

/// Minimal HTML escaping for safe text interpolation.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Very small markdown → HTML converter covering the subset produced by the
/// summary engine: headings (#, ##, ###), bold **x**, italic *x*, and paragraphs.
/// This avoids pulling in a full markdown crate for export rendering.
fn markdown_to_html(md: &str) -> String {
    let mut out = String::new();
    let mut in_paragraph = false;

    for line in md.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if in_paragraph {
                out.push_str("</p>\n");
                in_paragraph = false;
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("### ") {
            if in_paragraph {
                out.push_str("</p>\n");
                in_paragraph = false;
            }
            out.push_str(&format!("<h3>{}</h3>\n", inline_md(rest)));
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            if in_paragraph {
                out.push_str("</p>\n");
                in_paragraph = false;
            }
            out.push_str(&format!("<h2>{}</h2>\n", inline_md(rest)));
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            if in_paragraph {
                out.push_str("</p>\n");
                in_paragraph = false;
            }
            out.push_str(&format!("<h1>{}</h1>\n", inline_md(rest)));
        } else {
            if !in_paragraph {
                out.push_str("<p>");
                in_paragraph = true;
            } else {
                out.push(' ');
            }
            out.push_str(&inline_md(trimmed));
        }
    }
    if in_paragraph {
        out.push_str("</p>\n");
    }
    out
}

/// Inline markdown: **bold** and *italic* → HTML.
fn inline_md(text: &str) -> String {
    let escaped = escape_html(text);
    // bold
    let bolded = regex_replace(&escaped, "\\*\\*([^*]+)\\*\\*", "<strong>$1</strong>");
    // italic (single asterisk, not part of bold)
    regex_replace(&bolded, "(?<!\\*)\\*([^*]+)\\*(?!\\*)", "<em>$1</em>")
}

/// Tiny regex-free replacement helper used by `inline_md`.
/// Replaces every occurrence of `pattern` (literal `**` / `*` only) — kept simple
/// to avoid an extra dependency for export formatting.
fn regex_replace(input: &str, _pattern: &str, _replacement: &str) -> String {
    // Fallback: perform naive replacements for the two known patterns.
    // Bold
    let mut s = input.replace("**", "\u{0}bold\u{0}");
    // Italic (single *)
    s = s.replace('*', "\u{0}em\u{0}");
    // Reconstruct alternating open/close tags
    let parts: Vec<&str> = s.split('\u{0}').collect();
    let mut out = String::new();
    let mut i = 0;
    let mut bold_open = false;
    let mut em_open = false;
    while i < parts.len() {
        let part = parts[i];
        if part == "bold" {
            out.push_str(if bold_open { "</strong>" } else { "<strong>" });
            bold_open = !bold_open;
        } else if part == "em" {
            out.push_str(if em_open { "</em>" } else { "<em>" });
            em_open = !em_open;
        } else {
            out.push_str(part);
        }
        i += 1;
    }
    out
}

/// Gets the meeting markdown as a string without saving to file.
#[tauri::command]
pub async fn api_get_meeting_markdown(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    format: Option<String>,
) -> Result<String, String> {
    log_info!(
        "api_get_meeting_markdown called for meeting_id: {}",
        meeting_id
    );
    let pool = state.db_manager.pool();
    let section = format
        .as_deref()
        .map(ExportSection::from_str)
        .unwrap_or_default();
    build_meeting_markdown_ex(pool, &meeting_id, section).await
}

/// Gets the meeting HTML as a string without saving to file.
#[tauri::command]
pub async fn api_get_meeting_html(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    format: Option<String>,
) -> Result<String, String> {
    log_info!(
        "api_get_meeting_html called for meeting_id: {}",
        meeting_id
    );
    let pool = state.db_manager.pool();
    let section = format
        .as_deref()
        .map(ExportSection::from_str)
        .unwrap_or_default();
    build_meeting_html_ex(pool, &meeting_id, section).await
}

/// Exports a meeting to a markdown file via a save dialog.
#[tauri::command]
pub async fn api_export_meeting_markdown<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    format: Option<String>,
) -> Result<ExportResult, String> {
    let section = format
        .as_deref()
        .map(ExportSection::from_str)
        .unwrap_or_default();
    export_meeting_to_file(app, state, &meeting_id, "md", "Markdown", section, build_meeting_markdown_ex).await
}

/// Exports a meeting to an HTML file via a save dialog (print-friendly, self-contained).
#[tauri::command]
pub async fn api_export_meeting_html<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    format: Option<String>,
) -> Result<ExportResult, String> {
    let section = format
        .as_deref()
        .map(ExportSection::from_str)
        .unwrap_or_default();
    export_meeting_to_file(app, state, &meeting_id, "html", "HTML", section, build_meeting_html_ex).await
}

/// Shared file-export routine for markdown/HTML renderers.
async fn export_meeting_to_file<R, F, Fut>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: &str,
    ext: &str,
    filter_label: &str,
    section: ExportSection,
    renderer: F,
) -> Result<ExportResult, String>
where
    R: tauri::Runtime,
    F: FnOnce(&sqlx::SqlitePool, &str, ExportSection) -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    log_info!(
        "api_export_meeting_{} called for meeting_id: {}",
        ext, meeting_id
    );

    let pool = state.db_manager.pool();
    let content = renderer(pool, meeting_id, section).await?;

    let meeting = MeetingsRepository::get_meeting(pool, meeting_id)
        .await
        .map_err(|e| format!("Failed to fetch meeting: {}", e))?
        .ok_or_else(|| format!("Meeting {} not found", meeting_id))?;

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

    let file_path = app
        .dialog()
        .file()
        .add_filter(filter_label, &[ext])
        .set_file_name(&format!("{}.{}", safe_title, ext))
        .blocking_save_file();

    let file_path = match file_path {
        Some(path) => path.to_string(),
        None => {
            log_info!("User cancelled export dialog");
            return Err("User cancelled export".to_string());
        }
    };

    std::fs::write(&file_path, &content).map_err(|e| {
        let msg = format!("Failed to write file: {}", e);
        log_error!("{}", msg);
        msg
    })?;

    let bytes_written = content.len() as u64;
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

/// Which parts of a meeting to include in the export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportSection {
    /// Full document: header, summary, and transcript.
    #[default]
    Full,
    /// Only the AI summary.
    SummaryOnly,
    /// Only the transcript segments.
    TranscriptOnly,
}

impl ExportSection {
    /// Parse from a user-facing string ("full" | "summary" | "transcript").
    fn from_str(s: &str) -> ExportSection {
        match s.to_lowercase().as_str() {
            "summary" | "summary_only" => ExportSection::SummaryOnly,
            "transcript" | "transcript_only" => ExportSection::TranscriptOnly,
            _ => ExportSection::Full,
        }
    }
}

/// Markdown renderer with a configurable section filter.
async fn build_meeting_markdown_ex(
    pool: &sqlx::SqlitePool,
    meeting_id: &str,
    section: ExportSection,
) -> Result<String, String> {
    let data = fetch_export_data(pool, meeting_id).await?;

    let mut md = String::new();
    md.push_str(&format!("# {}\n\n", data.title));
    md.push_str(&format!(
        "**Date:** {}\n\n",
        data.created_at.0.format("%Y-%m-%d %H:%M")
    ));

    if section != ExportSection::TranscriptOnly {
        md.push_str("---\n\n");
        md.push_str("## Summary\n\n");
        if let Some(ref summary) = data.summary_markdown {
            md.push_str(summary);
            md.push('\n');
        } else {
            md.push_str("_No summary available._\n");
        }
    }

    if section != ExportSection::SummaryOnly {
        if section == ExportSection::Full {
            md.push_str("\n---\n\n");
        } else if section == ExportSection::TranscriptOnly {
            md.push('\n');
        }
        md.push_str("## Transcript\n\n");
        if data.transcripts.is_empty() {
            md.push_str("_No transcript data available._\n");
        } else {
            for (idx, segment) in data.transcripts.iter().enumerate() {
                let segment_num = idx + 1;
                md.push_str(&format!(
                    "### Segment {} — {}\n\n",
                    segment_num,
                    format_segment_timestamp(segment)
                ));
                md.push_str(&segment.transcript);
                md.push_str("\n\n");
            }
        }
    }

    Ok(md)
}

/// HTML renderer with a configurable section filter.
async fn build_meeting_html_ex(
    pool: &sqlx::SqlitePool,
    meeting_id: &str,
    section: ExportSection,
) -> Result<String, String> {
    let data = fetch_export_data(pool, meeting_id).await?;

    let date_str = data.created_at.0.format("%Y-%m-%d %H:%M").to_string();
    let summary_html = match &data.summary_markdown {
        Some(s) if section != ExportSection::TranscriptOnly => markdown_to_html(s),
        _ if section == ExportSection::TranscriptOnly => String::new(),
        None => "<p class=\"muted\">No summary available.</p>".to_string(),
    };

    let transcript_html = if section == ExportSection::SummaryOnly {
        String::new()
    } else if data.transcripts.is_empty() {
        "<p class=\"muted\">No transcript data available.</p>".to_string()
    } else {
        let mut segments = String::new();
        for (idx, segment) in data.transcripts.iter().enumerate() {
            let segment_num = idx + 1;
            let ts = format_segment_timestamp(segment);
            segments.push_str(&format!(
                "<div class=\"segment\">\n  <div class=\"seg-meta\">Segment {} — {}</div>\n  <div class=\"seg-text\">{}</div>\n</div>\n",
                segment_num,
                ts,
                escape_html(&segment.transcript)
            ));
        }
        segments
    };

    let body = match section {
        ExportSection::SummaryOnly => format!(
            "<h2>Summary</h2>\n{}",
            if summary_html.is_empty() {
                "<p class=\"muted\">No summary available.</p>".to_string()
            } else {
                summary_html
            }
        ),
        ExportSection::TranscriptOnly => format!("<h2>Transcript</h2>\n{}", transcript_html),
        ExportSection::Full => format!(
            "<hr />\n<h2>Summary</h2>\n{}\n<hr />\n<h2>Transcript</h2>\n{}",
            if summary_html.is_empty() {
                "<p class=\"muted\">No summary available.</p>".to_string()
            } else {
                summary_html
            },
            transcript_html
        ),
    };

    Ok(format!(
        "<!DOCTYPE html>\n\
<html lang=\"en\">\n\
<head>\n\
  <meta charset=\"utf-8\" />\n\
  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\" />\n\
  <title>{}</title>\n\
  <style>\n\
    :root {{ --fg: #1a1a1a; --muted: #6b7280; --accent: #2563eb; --border: #e5e7eb; }}\n\
    * {{ box-sizing: border-box; }}\n\
    body {{ font-family: -apple-system, BlinkMacSystemFont, \"Segoe UI\", Roboto, sans-serif; color: var(--fg); max-width: 820px; margin: 2rem auto; padding: 0 1.5rem; line-height: 1.6; }}\n\
    h1 {{ font-size: 1.8rem; margin-bottom: 0.25rem; }}\n\
    .date {{ color: var(--muted); font-size: 0.95rem; margin-bottom: 1.5rem; }}\n\
    h2 {{ border-bottom: 2px solid var(--border); padding-bottom: 0.4rem; margin-top: 2rem; }}\n\
    .muted {{ color: var(--muted); font-style: italic; }}\n\
    .segment {{ border-left: 3px solid var(--accent); padding: 0.5rem 0 0.5rem 1rem; margin: 1rem 0; }}\n\
    .seg-meta {{ font-size: 0.8rem; color: var(--muted); margin-bottom: 0.25rem; }}\n\
    .seg-text {{ white-space: pre-wrap; }}\n\
    hr {{ border: none; border-top: 1px solid var(--border); margin: 2rem 0; }}\n\
    @media print {{ body {{ margin: 0; }} h2 {{ page-break-after: avoid; }} .segment {{ page-break-inside: avoid; }} }}\n\
  </style>\n\
</head>\n\
<body>\n\
  <h1>{}</h1>\n\
  <div class=\"date\">Date: {}</div>\n\
  {}\n\
</body>\n\
</html>\n",
        escape_html(&data.title),
        escape_html(&data.title),
        date_str,
        body
    ))
}

/// Combine multiple meetings into one markdown document.
async fn build_batch_markdown(
    pool: &sqlx::SqlitePool,
    meeting_ids: &[String],
    section: ExportSection,
) -> Result<String, String> {
    let mut out = String::new();
    for (i, id) in meeting_ids.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n---\n\n");
        }
        let single = build_meeting_markdown_ex(pool, id, section).await?;
        out.push_str(&single);
    }
    Ok(out)
}

/// Combine multiple meetings into one HTML document.
async fn build_batch_html(
    pool: &sqlx::SqlitePool,
    meeting_ids: &[String],
    section: ExportSection,
) -> Result<String, String> {
    let mut sections = String::new();
    for id in meeting_ids {
        let single = build_meeting_html_ex(pool, id, section).await?;
        // Extract the <body> inner content of each meeting doc.
        if let Some(start) = single.find("<body>") {
            if let Some(end) = single.find("</body>") {
                let inner = &single[start + "<body>".len()..end];
                sections.push_str(inner);
                sections.push_str("\n<hr />\n");
            }
        }
    }
    Ok(format!(
        "<!DOCTYPE html>\n<html lang=\"en\"><head><meta charset=\"utf-8\" />\
<style>body{{font-family:sans-serif;max-width:820px;margin:2rem auto;padding:0 1.5rem;line-height:1.6;}}\
h1{{font-size:1.8rem;}}hr{{border:none;border-top:1px solid #e5e7eb;margin:2rem 0;}}\
.segment{{border-left:3px solid #2563eb;padding:0.5rem 0 0.5rem 1rem;margin:1rem 0;}}\
.seg-meta{{font-size:0.8rem;color:#6b7280;}}</style></head>\
<body>\n{}</body></html>",
        sections
    ))
}

/// Gets the combined markdown for multiple meetings.
#[tauri::command]
pub async fn api_get_meetings_markdown(
    state: tauri::State<'_, AppState>,
    meeting_ids: Vec<String>,
    format: Option<String>,
) -> Result<String, String> {
    let pool = state.db_manager.pool();
    let section = format
        .as_deref()
        .map(ExportSection::from_str)
        .unwrap_or_default();
    build_batch_markdown(pool, &meeting_ids, section).await
}

/// Gets the combined HTML for multiple meetings.
#[tauri::command]
pub async fn api_get_meetings_html(
    state: tauri::State<'_, AppState>,
    meeting_ids: Vec<String>,
    format: Option<String>,
) -> Result<String, String> {
    let pool = state.db_manager.pool();
    let section = format
        .as_deref()
        .map(ExportSection::from_str)
        .unwrap_or_default();
    build_batch_html(pool, &meeting_ids, section).await
}

/// Exports multiple meetings to a single markdown file via a save dialog.
#[tauri::command]
pub async fn api_export_meetings_markdown<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_ids: Vec<String>,
    format: Option<String>,
) -> Result<ExportResult, String> {
    let section = format
        .as_deref()
        .map(ExportSection::from_str)
        .unwrap_or_default();
    export_batch_to_file(app, state, &meeting_ids, "md", "Markdown", section, build_meeting_markdown_ex).await
}

/// Exports multiple meetings to a single HTML file via a save dialog.
#[tauri::command]
pub async fn api_export_meetings_html<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_ids: Vec<String>,
    format: Option<String>,
) -> Result<ExportResult, String> {
    let section = format
        .as_deref()
        .map(ExportSection::from_str)
        .unwrap_or_default();
    export_batch_to_file(app, state, &meeting_ids, "html", "HTML", section, build_meeting_html_ex).await
}

/// Shared file-export routine for batch (multi-meeting) renderers.
async fn export_batch_to_file<R, F, Fut>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_ids: &[String],
    ext: &str,
    filter_label: &str,
    section: ExportSection,
    renderer: F,
) -> Result<ExportResult, String>
where
    R: tauri::Runtime,
    F: FnOnce(&sqlx::SqlitePool, &str, ExportSection) -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    if meeting_ids.is_empty() {
        return Err("No meetings selected for export".to_string());
    }

    let pool = state.db_manager.pool();
    let mut content = String::new();
    for (i, id) in meeting_ids.iter().enumerate() {
        if i > 0 {
            content.push_str("\n\n---\n\n");
        }
        let single = renderer(pool, id, section).await?;
        content.push_str(&single);
    }

    let file_path = app
        .dialog()
        .file()
        .add_filter(filter_label, &[ext])
        .set_file_name(&format!("meetings_export.{}", ext))
        .blocking_save_file();

    let file_path = match file_path {
        Some(path) => path.to_string(),
        None => {
            log_info!("User cancelled batch export dialog");
            return Err("User cancelled export".to_string());
        }
    };

    std::fs::write(&file_path, &content).map_err(|e| {
        let msg = format!("Failed to write file: {}", e);
        log_error!("{}", msg);
        msg
    })?;

    let bytes_written = content.len() as u64;
    log_info!(
        "Exported {} meetings to {} ({} bytes)",
        meeting_ids.len(),
        file_path,
        bytes_written
    );

    Ok(ExportResult {
        file_path,
        success: true,
        bytes_written,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_test_db() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory pool");

        sqlx::query(
            "CREATE TABLE meetings (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                folder_path TEXT
            );",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE transcripts (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                transcript TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                summary TEXT,
                action_items TEXT,
                key_points TEXT,
                audio_start_time REAL,
                audio_end_time REAL,
                duration REAL
            );",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE summary_processes (
                meeting_id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                error TEXT,
                result TEXT,
                start_time TEXT,
                end_time TEXT,
                chunk_count INTEGER DEFAULT 0,
                processing_time REAL DEFAULT 0.0,
                metadata TEXT
            );",
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    async fn insert_meeting(pool: &sqlx::SqlitePool, id: &str, title: &str, created_at: &str) {
        sqlx::query(
            "INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(id)
        .bind(title)
        .bind(created_at)
        .bind(created_at)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_transcript(
        pool: &sqlx::SqlitePool,
        id: &str,
        meeting_id: &str,
        text: &str,
        timestamp: &str,
        audio_start_time: Option<f64>,
    ) {
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(meeting_id)
        .bind(text)
        .bind(timestamp)
        .bind(audio_start_time)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_summary(pool: &sqlx::SqlitePool, meeting_id: &str, markdown: &str) {
        let result_json = serde_json::json!({ "markdown": markdown }).to_string();
        sqlx::query(
            "INSERT INTO summary_processes (meeting_id, status, created_at, updated_at, result)
             VALUES (?, 'completed', ?, ?, ?)",
        )
        .bind(meeting_id)
        .bind("2024-01-01T10:00:00Z")
        .bind("2024-01-01T10:01:00Z")
        .bind(result_json)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_build_meeting_markdown_full() {
        let pool = setup_test_db().await;
        insert_meeting(&pool, "m1", "Sprint Planning", "2024-01-15T09:30:00Z").await;
        insert_transcript(&pool, "t1", "m1", "We discussed the roadmap.", "2024-01-15T09:30:00Z", Some(0.0)).await;
        insert_transcript(&pool, "t2", "m1", "Alice will own the auth module.", "2024-01-15T09:35:00Z", Some(125.5)).await;
        insert_summary(&pool, "m1", "## Key Decisions\n- Ship v2 in Q2").await;

        let md = build_meeting_markdown(&pool, "m1").await.unwrap();
        assert!(md.starts_with("# Sprint Planning\n\n"), "got: {}", md);
        assert!(md.contains("**Date:** 2024-01-15 09:30"));
        assert!(md.contains("## Summary"));
        assert!(md.contains("## Key Decisions"));
        assert!(md.contains("- Ship v2 in Q2"));
        assert!(md.contains("## Transcript"));
        assert!(md.contains("### Segment 1 — 00:00"));
        assert!(md.contains("### Segment 2 — 02:05"));
        assert!(md.contains("We discussed the roadmap."));
        assert!(md.contains("Alice will own the auth module."));
        assert_eq!(md.matches("---").count(), 2);
    }

    #[tokio::test]
    async fn test_build_meeting_markdown_no_summary() {
        let pool = setup_test_db().await;
        insert_meeting(&pool, "m2", "Standup", "2024-02-01T08:00:00Z").await;
        insert_transcript(&pool, "t1", "m2", "Yesterday: X. Today: Y.", "2024-02-01T08:00:00Z", Some(10.0)).await;

        let md = build_meeting_markdown(&pool, "m2").await.unwrap();
        assert!(md.contains("# Standup"));
        assert!(md.contains("_No summary available._"));
        assert!(md.contains("### Segment 1 — 00:10"));
    }

    #[tokio::test]
    async fn test_build_meeting_markdown_no_transcripts() {
        let pool = setup_test_db().await;
        insert_meeting(&pool, "m3", "Empty Meeting", "2024-03-10T12:00:00Z").await;
        insert_summary(&pool, "m3", "Auto-generated notes.").await;

        let md = build_meeting_markdown(&pool, "m3").await.unwrap();
        assert!(md.contains("# Empty Meeting"));
        assert!(md.contains("## Transcript"));
        assert!(md.contains("_No transcript data available._"));
    }

    #[tokio::test]
    async fn test_build_meeting_markdown_missing_meeting_errors() {
        let pool = setup_test_db().await;
        let result = build_meeting_markdown(&pool, "ghost").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_build_meeting_markdown_corrupt_summary_json() {
        let pool = setup_test_db().await;
        insert_meeting(&pool, "m4", "Corrupt", "2024-04-01T00:00:00Z").await;
        sqlx::query(
            "INSERT INTO summary_processes (meeting_id, status, created_at, updated_at, result)
             VALUES (?, 'completed', ?, ?, ?)",
        )
        .bind("m4")
        .bind("2024-04-01T00:00:00Z")
        .bind("2024-04-01T00:01:00Z")
        .bind("not-valid-json")
        .execute(&pool)
        .await
        .unwrap();

        let md = build_meeting_markdown(&pool, "m4").await.unwrap();
        assert!(md.contains("_No summary available._"));
    }

    #[tokio::test]
    async fn test_build_meeting_markdown_summary_missing_markdown_field() {
        let pool = setup_test_db().await;
        insert_meeting(&pool, "m5", "NoMarkdown", "2024-05-01T00:00:00Z").await;
        let result_json = serde_json::json!({ "english_cache": "something" }).to_string();
        sqlx::query(
            "INSERT INTO summary_processes (meeting_id, status, created_at, updated_at, result)
             VALUES (?, 'completed', ?, ?, ?)",
        )
        .bind("m5")
        .bind("2024-05-01T00:00:00Z")
        .bind("2024-05-01T00:01:00Z")
        .bind(result_json)
        .execute(&pool)
        .await
        .unwrap();

        let md = build_meeting_markdown(&pool, "m5").await.unwrap();
        assert!(md.contains("_No summary available._"));
    }

    #[tokio::test]
    async fn test_build_meeting_html_full() {
        let pool = setup_test_db().await;
        insert_meeting(&pool, "h1", "Q2 Planning", "2024-06-01T10:00:00Z").await;
        insert_transcript(&pool, "t1", "h1", "Intro <script> & discuss.", "2024-06-01T10:00:00Z", Some(0.0)).await;
        insert_summary(&pool, "h1", "## Highlights\n- **Launch** soon").await;

        let html = build_meeting_html(&pool, "h1").await.unwrap();
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<title>Q2 Planning</title>"));
        assert!(html.contains("Date: 2024-06-01 10:00"));
        assert!(html.contains("<h2>Summary</h2>"));
        // XSS safety: raw <script> must be escaped
        assert!(!html.contains("<script> & discuss."));
        assert!(html.contains("&lt;script&gt;"));
        // Bold markdown rendered
        assert!(html.contains("<strong>Launch</strong>"));
        assert!(html.contains("<h2>Transcript</h2>"));
        assert!(html.contains("Segment 1 — 00:00"));
    }

    #[tokio::test]
    async fn test_escape_html_basic() {
        assert_eq!(escape_html("<a>&\"b\""), "&lt;a&gt;&amp;&quot;b&quot;");
    }

    #[tokio::test]
    async fn test_build_meeting_markdown_summary_only() {
        let pool = setup_test_db().await;
        insert_meeting(&pool, "so1", "Planning", "2024-07-01T09:00:00Z").await;
        insert_transcript(&pool, "t1", "so1", "Hidden transcript text.", "2024-07-01T09:00:00Z", Some(0.0)).await;
        insert_summary(&pool, "so1", "## Summary\nTop points.").await;

        let md = build_meeting_markdown_ex(&pool, "so1", ExportSection::SummaryOnly)
            .await
            .unwrap();
        assert!(md.contains("## Summary"));
        assert!(md.contains("Top points."));
        assert!(!md.contains("## Transcript"));
        assert!(!md.contains("Hidden transcript text."));
        // Only one separator on either side of the summary section.
        assert!(md.matches("---").count() <= 1);
    }

    #[tokio::test]
    async fn test_build_meeting_markdown_transcript_only() {
        let pool = setup_test_db().await;
        insert_meeting(&pool, "to1", "Planning", "2024-07-02T09:00:00Z").await;
        insert_transcript(&pool, "t1", "to1", "Visible transcript text.", "2024-07-02T09:00:00Z", Some(0.0)).await;
        insert_summary(&pool, "to1", "## Summary\nShould be hidden.").await;

        let md = build_meeting_markdown_ex(&pool, "to1", ExportSection::TranscriptOnly)
            .await
            .unwrap();
        assert!(!md.contains("## Summary"));
        assert!(!md.contains("Should be hidden."));
        assert!(md.contains("## Transcript"));
        assert!(md.contains("Visible transcript text."));
    }

    #[tokio::test]
    async fn test_build_batch_markdown_combines_meetings() {
        let pool = setup_test_db().await;
        insert_meeting(&pool, "b1", "Meeting One", "2024-08-01T09:00:00Z").await;
        insert_transcript(&pool, "t1", "b1", "One notes.", "2024-08-01T09:00:00Z", Some(0.0)).await;
        insert_summary(&pool, "b1", "## Summary\nFirst.").await;
        insert_meeting(&pool, "b2", "Meeting Two", "2024-08-02T09:00:00Z").await;
        insert_transcript(&pool, "t2", "b2", "Two notes.", "2024-08-02T09:00:00Z", Some(0.0)).await;
        insert_summary(&pool, "b2", "## Summary\nSecond.").await;

        let md = build_batch_markdown(&pool, &["b1".to_string(), "b2".to_string()], ExportSection::Full)
            .await
            .unwrap();
        assert!(md.contains("# Meeting One"));
        assert!(md.contains("# Meeting Two"));
        assert!(md.contains("First."));
        assert!(md.contains("Second."));
        // Separator between the two meetings.
        assert!(md.contains("\n\n---\n\n"));
    }

    #[test]
    fn test_export_section_from_str() {
        assert_eq!(ExportSection::from_str("full"), ExportSection::Full);
        assert_eq!(ExportSection::from_str("SUMMARY"), ExportSection::SummaryOnly);
        assert_eq!(ExportSection::from_str("transcript_only"), ExportSection::TranscriptOnly);
        // Unknown -> defaults to Full.
        assert_eq!(ExportSection::from_str("weird"), ExportSection::Full);
    }
}
