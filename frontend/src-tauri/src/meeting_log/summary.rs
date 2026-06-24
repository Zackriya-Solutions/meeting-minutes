//! Session finalization: ask the local summary model for a structured JSON
//! summary, render it to markdown, write `{time}_summary.md`, and append a
//! section to the per-day `summary.md`. Then trigger memory indexing.

use super::config::config;
use super::session::SessionLog;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActionItem {
    #[serde(default)]
    pub assignee: String,
    #[serde(default)]
    pub task: String,
    #[serde(default = "default_status")]
    pub status: String,
}

fn default_status() -> String {
    "open".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StructuredSummary {
    #[serde(default)]
    pub overview: Vec<String>,
    #[serde(default)]
    pub action_items: Vec<ActionItem>,
    #[serde(default)]
    pub topics: Vec<String>,
}

const SUMMARY_SYSTEM: &str = "You are a meeting summarizer for Thai/English code-switched engineering meetings. \
Keep technical terms in English exactly as spoken (Kafka, ACL, gRPC, consumer lag, Vault Core, Redis, partition, broker, deploy, UAT). \
Do not translate or transliterate technical terms. Respond with JSON only, no prose.";

/// Build the JSON-only prompt for the summary model.
fn build_prompt(transcript: &str) -> String {
    format!(
        "{SUMMARY_SYSTEM}\n\nSummarize the following meeting transcript.\n\
Return ONLY a JSON object with this exact shape:\n\
{{\n  \"overview\": [\"short bullet\", ...],\n  \"action_items\": [{{\"assignee\": \"name or Team\", \"task\": \"...\", \"status\": \"open\"}}],\n  \"topics\": [\"Kafka\", \"ACL\", ...]\n}}\n\n\
Transcript:\n{transcript}\n"
    )
}

/// Call Ollama `/api/generate` with `format: json` and parse the result.
async fn request_summary(transcript: &str) -> Result<StructuredSummary, String> {
    let cfg = config();
    let model = super::models::resolve_summary_model().await;
    let url = format!("{}/api/generate", cfg.ollama_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "prompt": build_prompt(transcript),
        "stream": false,
        "format": "json",
        "options": { "temperature": 0.2 }
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("summary request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("summary model returned {}", resp.status()));
    }

    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("summary decode failed: {e}"))?;
    let inner = v
        .get("response")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .trim();

    serde_json::from_str::<StructuredSummary>(inner)
        .map_err(|e| format!("summary JSON parse failed: {e}; raw: {inner}"))
}

/// Render the structured summary to the markdown body used in both the
/// per-session file and (a condensed form of) the daily log.
fn render_markdown(summary: &StructuredSummary, started_human: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Summary — {}\n\n", started_human));

    out.push_str("## Overview\n");
    if summary.overview.is_empty() {
        out.push_str("- (none)\n");
    } else {
        for o in &summary.overview {
            out.push_str(&format!("- {}\n", o.trim()));
        }
    }

    out.push_str("\n## Action Items\n");
    if summary.action_items.is_empty() {
        out.push_str("- (none)\n");
    } else {
        // Group tasks under assignee.
        let mut by_assignee: Vec<(String, Vec<&ActionItem>)> = Vec::new();
        for item in &summary.action_items {
            let key = if item.assignee.trim().is_empty() {
                "Team".to_string()
            } else {
                item.assignee.trim().to_string()
            };
            if let Some(entry) = by_assignee.iter_mut().find(|(a, _)| a == &key) {
                entry.1.push(item);
            } else {
                by_assignee.push((key, vec![item]));
            }
        }
        for (assignee, items) in by_assignee {
            out.push_str(&format!("- {}\n", assignee));
            for item in items {
                let status = if item.status.trim().is_empty() {
                    "open"
                } else {
                    item.status.trim()
                };
                out.push_str(&format!("  - {} ({})\n", item.task.trim(), status));
            }
        }
    }

    out.push_str("\n## Topics\n");
    if summary.topics.is_empty() {
        out.push_str("(none)\n");
    } else {
        out.push_str(&summary.topics.join(", "));
        out.push('\n');
    }

    out
}

/// Result of finalizing a session, returned for logging / frontend events.
#[derive(Debug, Clone, Serialize)]
pub struct FinalizeResult {
    pub summary_path: String,
    pub daily_log_path: String,
    pub transcript_path: String,
    pub topics: Vec<String>,
}

/// Generate the summary, write the per-session summary file, append the daily
/// log, and kick off memory indexing. Designed to never panic; returns an
/// error string the caller can log.
pub async fn finalize(session: SessionLog) -> Result<FinalizeResult, String> {
    let cfg = config();
    let transcript = session.transcript_plain();

    if transcript.trim().is_empty() {
        return Err("no transcript content to summarize".to_string());
    }

    // 1. Structured summary (fall back to a minimal summary if the model fails).
    let summary = match request_summary(&transcript).await {
        Ok(s) => s,
        Err(e) => {
            log::warn!("meeting_log: summary model failed ({e}); writing fallback summary");
            StructuredSummary {
                overview: vec![format!(
                    "Transcript captured ({} segments). Summary model unavailable.",
                    session.segments.len()
                )],
                ..Default::default()
            }
        }
    };

    let summary_md = render_markdown(&summary, &session.started_human);

    // 2. Per-session summary file.
    let summary_name = cfg.summary_pattern.replace("{time}", &session.session_time);
    let summary_path = session.date_folder.join(&summary_name);
    fs::write(&summary_path, &summary_md)
        .map_err(|e| format!("write summary {:?}: {e}", summary_path))?;

    // 3. Append to the per-day daily log.
    let daily_path = session.date_folder.join(&cfg.daily_log);
    append_daily_log(&daily_path, &session, &summary, &summary_name)?;

    // 4. Fire-and-forget memory indexing via the sidecar.
    let index_session = session.clone();
    let summary_for_index = summary.clone();
    tokio::spawn(async move {
        if let Err(e) =
            super::memory::index_session(&index_session, &summary_for_index).await
        {
            log::warn!("meeting_log: memory indexing skipped/failed: {e}");
        }
    });

    Ok(FinalizeResult {
        summary_path: summary_path.to_string_lossy().to_string(),
        daily_log_path: daily_path.to_string_lossy().to_string(),
        transcript_path: session.transcript_path.to_string_lossy().to_string(),
        topics: summary.topics,
    })
}

fn append_daily_log(
    daily_path: &PathBuf,
    session: &SessionLog,
    summary: &StructuredSummary,
    summary_name: &str,
) -> Result<(), String> {
    // Title: session clock time + first topic (or meeting name).
    let clock = session
        .session_time
        .replace('-', ":"); // "14-30-05" -> "14:30:05"
    let title = summary
        .topics
        .first()
        .cloned()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| session.meeting_name.clone());

    let mut section = String::new();
    section.push_str(&format!("\n---\n## {} — {}\n", clock, title));
    if summary.overview.is_empty() {
        section.push_str("- (no overview)\n");
    } else {
        for o in &summary.overview {
            section.push_str(&format!("- {}\n", o.trim()));
        }
    }
    section.push_str(&format!("→ full: {}\n", summary_name));

    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(daily_path)
        .map_err(|e| format!("open daily log {:?}: {e}", daily_path))?;
    f.write_all(section.as_bytes())
        .and_then(|_| f.flush())
        .map_err(|e| format!("append daily log: {e}"))?;
    Ok(())
}
