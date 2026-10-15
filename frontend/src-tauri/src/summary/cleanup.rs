//! Optional transcript cleanup. One pass after speech-to-text corrects names and terms.
//!
//! The speech engine is unchanged. This reads saved transcript rows, asks the selected
//! summary-style model to fix spellings, then applies terminology replacements.

use crate::database::repositories::setting::SettingsRepository;
use crate::state::AppState;
use crate::summary::llm_client::{generate_summary, LLMProvider};
use crate::summary::service::resolve_llm;
use crate::summary::terminology::{apply_corrections, load_entries, TerminologyEntry};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::collections::{HashMap, HashSet};
use tauri::{AppHandle, Manager, Runtime};

pub const MAX_CLEANUP_PROMPT_CHARS: usize = 8_000;

const SYSTEM_PROMPT: &str = "\
You correct speech-to-text names and terms.
Return every input segment.
Keep the same [id], speaker label, order, and wording.
Change only misheard names, product names, and technical terms using the correction guide.
Do not summarize, delete, merge, or add segments.
Output only the segments, one per line, in the form [id] speaker: text.";

/// Saved cleanup settings. `enabled` defaults to false.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupConfig {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub ollama_endpoint: Option<String>,
    pub prompt: String,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: String::new(),
            model: String::new(),
            ollama_endpoint: None,
            prompt: String::new(),
        }
    }
}

/// Row shape for the cleanup columns on `settings`.
#[derive(Debug, sqlx::FromRow)]
pub struct CleanupConfigRow {
    #[sqlx(rename = "cleanupEnabled")]
    pub enabled: bool,
    #[sqlx(rename = "cleanupProvider")]
    pub provider: Option<String>,
    #[sqlx(rename = "cleanupModel")]
    pub model: Option<String>,
    #[sqlx(rename = "cleanupOllamaEndpoint")]
    pub ollama_endpoint: Option<String>,
    #[sqlx(rename = "cleanupPrompt")]
    pub prompt: String,
}

impl From<CleanupConfigRow> for CleanupConfig {
    fn from(row: CleanupConfigRow) -> Self {
        Self {
            enabled: row.enabled,
            provider: row.provider.unwrap_or_default(),
            model: row.model.unwrap_or_default(),
            ollama_endpoint: row.ollama_endpoint.filter(|value| !value.trim().is_empty()),
            prompt: row.prompt,
        }
    }
}

/// Result of one cleanup run.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupOutcome {
    pub updated: usize,
    pub kept: usize,
    pub skipped: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct SourceLine {
    id: String,
    speaker: Option<String>,
    /// Text sent to the model. This is the speech-to-text original once one exists.
    source: String,
    /// Text currently shown. Unchanged lines are not written again.
    current: String,
}

/// Rejects a prompt over the settings limit. Surrounding whitespace is not counted.
pub fn check_prompt_length(prompt: &str) -> Result<(), String> {
    let length = prompt.trim().chars().count();
    if length > MAX_CLEANUP_PROMPT_CHARS {
        return Err(format!(
            "Cleanup prompt is too long ({} characters). The limit is {}.",
            length, MAX_CLEANUP_PROMPT_CHARS
        ));
    }
    Ok(())
}

/// Parses model output into segment id → corrected text.
///
/// A duplicated id is dropped so that segment keeps its source wording.
/// Lines that are not `[id] speaker: text` are ignored.
pub fn parse_cleanup_output(output: &str) -> HashMap<String, String> {
    let mut accepted: HashMap<String, String> = HashMap::new();
    let mut duplicated: HashSet<String> = HashSet::new();

    for raw_line in strip_code_fence(output).lines() {
        let Some((id, text)) = parse_segment_line(raw_line) else {
            continue;
        };
        if duplicated.contains(&id) || accepted.contains_key(&id) {
            accepted.remove(&id);
            duplicated.insert(id);
            continue;
        }
        accepted.insert(id, text);
    }

    accepted
}

fn strip_code_fence(output: &str) -> String {
    let trimmed = output.trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed.to_string();
    };
    let rest = rest.trim_start_matches(|c: char| c.is_ascii_alphanumeric());
    let rest = rest.trim_start_matches(['\r', '\n']);
    rest.trim_end()
        .trim_end_matches("```")
        .trim()
        .to_string()
}

fn parse_segment_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    let rest = line.strip_prefix('[')?;
    let (id, after_id) = rest.split_once(']')?;
    let id = id.trim();
    if id.is_empty() {
        return None;
    }
    let text = after_id.trim().split_once(':')?.1.trim();
    if text.is_empty() {
        return None;
    }
    Some((id.to_string(), text.to_string()))
}

/// Off never reaches the model. On continues into the prompt and model checks.
fn skip_when_disabled(enabled: bool) -> Option<CleanupOutcome> {
    if enabled {
        None
    } else {
        Some(CleanupOutcome { updated: 0, kept: 0, skipped: true })
    }
}

/// Picks the model text when it is present, otherwise the source line, then applies terminology.
pub fn choose_text(source: &str, model_text: Option<&str>, entries: &[TerminologyEntry]) -> String {
    let chosen = model_text.map(str::trim).filter(|text| !text.is_empty()).unwrap_or(source);
    apply_corrections(chosen, entries).0
}

fn format_line(line: &SourceLine) -> String {
    let speaker = line
        .speaker
        .as_deref()
        .map(str::trim)
        .filter(|speaker| !speaker.is_empty())
        .unwrap_or("Speaker");
    let text = line.source.replace(['\n', '\r'], " ");
    format!("[{}] {}: {}", line.id, speaker, text)
}

/// Groups whole segments so a chunk never splits a line and segments are not repeated.
pub fn pack_lines(lines: &[SourceLine], max_chars: usize) -> Vec<Vec<usize>> {
    if lines.is_empty() {
        return Vec::new();
    }
    let limit = max_chars.max(1);
    let mut chunks = Vec::new();
    let mut current = Vec::new();
    let mut size = 0usize;

    for (index, line) in lines.iter().enumerate() {
        let line_len = format_line(line).chars().count() + 1;
        if !current.is_empty() && size.saturating_add(line_len) > limit {
            chunks.push(current);
            current = Vec::new();
            size = 0;
        }
        current.push(index);
        size = size.saturating_add(line_len);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn user_prompt(guide: &str, lines: &[&SourceLine]) -> String {
    let mut body = String::from("Correction guide:\n");
    body.push_str(guide.trim());
    body.push_str("\n\nSegments:\n");
    for line in lines {
        body.push_str(&format_line(line));
        body.push('\n');
    }
    body
}

/// Copies the current transcript into `transcript_original` the first time cleanup runs.
async fn snapshot_originals(pool: &SqlitePool, meeting_id: &str) -> Result<(), String> {
    sqlx::query(
        "UPDATE transcripts SET transcript_original = transcript
         WHERE meeting_id = ? AND transcript_original IS NULL",
    )
    .bind(meeting_id)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to keep the original transcript: {}", e))?;
    Ok(())
}

async fn load_lines(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<SourceLine>, String> {
    let rows = sqlx::query(
        "SELECT id, speaker, transcript, COALESCE(transcript_original, transcript) AS source
         FROM transcripts
         WHERE meeting_id = ?
         ORDER BY audio_start_time, timestamp, id",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to load transcript: {}", e))?;

    Ok(rows
        .into_iter()
        .map(|row| SourceLine {
            id: row.get("id"),
            speaker: row.get("speaker"),
            current: row.get("transcript"),
            source: row.get("source"),
        })
        .collect())
}

async fn write_changes(pool: &SqlitePool, meeting_id: &str, lines: &[(String, String)]) -> Result<(), String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| format!("Failed to save cleaned transcript: {}", e))?;
    for (id, text) in lines {
        sqlx::query("UPDATE transcripts SET transcript = ? WHERE meeting_id = ? AND id = ?")
            .bind(text)
            .bind(meeting_id)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to save cleaned transcript: {}", e))?;
    }
    tx.commit()
        .await
        .map_err(|e| format!("Failed to save cleaned transcript: {}", e))?;
    Ok(())
}

/// Cleans one meeting.
///
/// `enabled_override` is the retranscribe dialog switch. `None` uses the saved setting.
/// An off switch returns without calling the model. The original text is snapshotted
/// before the model runs, and later runs always correct from that snapshot.
pub async fn cleanup_meeting<R: Runtime>(
    app: &AppHandle<R>,
    meeting_id: &str,
    enabled_override: Option<bool>,
) -> Result<CleanupOutcome, String> {
    let pool = app.state::<AppState>().db_manager.pool().clone();
    let config = SettingsRepository::get_cleanup_config(&pool)
        .await
        .map_err(|e| format!("Failed to load cleanup settings: {}", e))?;
    let enabled = enabled_override.unwrap_or(config.enabled);
    if let Some(outcome) = skip_when_disabled(enabled) {
        info!("Transcript cleanup is off for meeting {}", meeting_id);
        return Ok(outcome);
    }
    if config.prompt.trim().is_empty() {
        return Err("Transcript cleanup is on, but the cleanup prompt is empty.".to_string());
    }
    if config.provider.trim().is_empty() || config.model.trim().is_empty() {
        return Err("Transcript cleanup is on, but no cleanup model is selected.".to_string());
    }

    snapshot_originals(&pool, meeting_id).await?;
    let lines = load_lines(&pool, meeting_id).await?;
    if lines.is_empty() {
        return Ok(CleanupOutcome { updated: 0, kept: 0, skipped: false });
    }

    let app_data_dir = app.path().app_data_dir().ok();
    let mut resolved = resolve_llm(&pool, &config.provider, &config.model, app_data_dir.clone()).await?;
    if resolved.provider == LLMProvider::Ollama {
        // Empty means the local default. Do not reuse the summary model's endpoint.
        resolved.ollama_endpoint = config.ollama_endpoint.clone();
    }

    let entries = app_data_dir.as_ref().map(|dir| load_entries(dir)).unwrap_or_default();
    let budget = resolved.context_tokens.saturating_mul(2).clamp(2_000, 24_000);
    let overhead = config.prompt.chars().count().saturating_add(800);
    let line_budget = budget.saturating_sub(overhead).max(500);
    let chunks = pack_lines(&lines, line_budget);

    let client = reqwest::Client::new();
    let mut parsed: HashMap<String, String> = HashMap::new();
    for indexes in chunks {
        let chunk_lines: Vec<&SourceLine> = indexes.iter().map(|index| &lines[*index]).collect();
        let prompt = user_prompt(&config.prompt, &chunk_lines);
        let completion = generate_summary(
            &client,
            &resolved.provider,
            &resolved.model,
            &resolved.api_key,
            SYSTEM_PROMPT,
            &prompt,
            resolved.ollama_endpoint.as_deref(),
            resolved.custom_openai_endpoint.as_deref(),
            resolved.max_tokens,
            resolved.temperature,
            resolved.top_p,
            resolved.app_data_dir.as_ref(),
            None,
        )
        .await?;
        parsed.extend(parse_cleanup_output(&completion.content));
    }

    let mut updates = Vec::new();
    let mut kept = 0usize;
    for line in &lines {
        let model_text = parsed.get(&line.id).map(String::as_str);
        if model_text.is_none() {
            warn!("Cleanup left segment {} unchanged", line.id);
        }
        let next = choose_text(&line.source, model_text, &entries);
        if next == line.current {
            kept += 1;
        } else {
            updates.push((line.id.clone(), next));
        }
    }

    let updated = updates.len();
    write_changes(&pool, meeting_id, &updates).await?;
    info!(
        "Transcript cleanup for {} updated {} segments and kept {}",
        meeting_id, updated, kept
    );
    Ok(CleanupOutcome { updated, kept, skipped: false })
}

#[tauri::command]
pub async fn api_get_cleanup_config<R: Runtime>(
    app: AppHandle<R>,
) -> Result<CleanupConfig, String> {
    let pool = app.state::<AppState>().db_manager.pool().clone();
    SettingsRepository::get_cleanup_config(&pool)
        .await
        .map_err(|e| format!("Failed to load cleanup settings: {}", e))
}

#[tauri::command]
pub async fn api_save_cleanup_config<R: Runtime>(
    app: AppHandle<R>,
    enabled: bool,
    provider: String,
    model: String,
    ollama_endpoint: Option<String>,
    prompt: String,
    api_key: Option<String>,
) -> Result<CleanupConfig, String> {
    check_prompt_length(&prompt)?;
    let config = CleanupConfig {
        enabled,
        provider: provider.trim().to_string(),
        model: model.trim().to_string(),
        ollama_endpoint: ollama_endpoint.filter(|value| !value.trim().is_empty()),
        prompt,
    };
    let pool = app.state::<AppState>().db_manager.pool().clone();
    SettingsRepository::save_cleanup_config(&pool, &config)
        .await
        .map_err(|e| format!("Failed to save cleanup settings: {}", e))?;
    if let Some(key) = api_key.filter(|key| !key.is_empty()) {
        if config.provider != "custom-openai" && config.provider != "builtin-ai" && config.provider != "ollama" {
            SettingsRepository::save_api_key(&pool, &config.provider, &key)
                .await
                .map_err(|e| format!("Failed to save cleanup API key: {}", e))?;
        }
    }
    Ok(config)
}

/// Cleans the meeting when the saved switch is on.
#[tauri::command]
pub async fn api_cleanup_meeting_transcript<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
) -> Result<CleanupOutcome, String> {
    cleanup_meeting(&app, &meeting_id, None).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::setting::SettingsRepository;
    use crate::database::test_support::{migrated_pool, seed_meeting, SeedRow};
    use crate::summary::terminology::TerminologyEntry;

    fn entry(wrong: &str, correct: &str) -> TerminologyEntry {
        TerminologyEntry { wrong: wrong.into(), correct: correct.into(), match_case: false }
    }

    #[test]
    fn disabled_cleanup_skips_before_any_model_call() {
        assert_eq!(
            skip_when_disabled(false),
            Some(CleanupOutcome { updated: 0, kept: 0, skipped: true })
        );
        assert!(skip_when_disabled(true).is_none());
    }

    #[test]
    fn rewritten_or_empty_segment_keeps_the_source_line() {
        let parsed = parse_cleanup_output("[other] Ada: Postgres\n[t1] Ada:\n");
        assert!(!parsed.contains_key("t1"));
        assert_eq!(choose_text("post gres", parsed.get("t1").map(String::as_str), &[]), "post gres");
    }

    #[test]
    fn parse_drops_duplicate_ids_and_keeps_a_single_correction() {
        let parsed = parse_cleanup_output(
            "```text\n[a] Ada: anomaly detection\n[a] Ada: again\n[b] Ben: Postgres\nnot a segment\n```",
        );
        assert!(!parsed.contains_key("a"));
        assert_eq!(parsed.get("b").map(String::as_str), Some("Postgres"));
    }

    #[test]
    fn empty_model_text_keeps_the_source_then_terminology() {
        let entries = [entry("post gres", "Postgres")];
        assert_eq!(choose_text("post gres jobs", None, &entries), "Postgres jobs");
        assert_eq!(choose_text("post gres jobs", Some("   "), &entries), "Postgres jobs");
        assert_eq!(
            choose_text("post gres jobs", Some("nom detection"), &[entry("nom detection", "anomaly detection")]),
            "anomaly detection"
        );
    }

    #[test]
    fn pack_lines_does_not_split_or_repeat_a_segment() {
        let lines = vec![
            SourceLine { id: "1".into(), speaker: None, source: "aaaa".into(), current: "aaaa".into() },
            SourceLine { id: "2".into(), speaker: None, source: "bbbb".into(), current: "bbbb".into() },
        ];
        let chunks = pack_lines(&lines, format_line(&lines[0]).chars().count());
        let flat: Vec<usize> = chunks.iter().flatten().copied().collect();
        assert_eq!(flat, vec![0, 1]);
        assert!(chunks.len() >= 2);
    }

    #[tokio::test]
    async fn saving_cleanup_leaves_the_summary_model_alone_and_defaults_off() {
        let pool = migrated_pool().await;
        SettingsRepository::save_model_config(&pool, "ollama", "llama3.2:latest", "large-v3", None)
            .await
            .unwrap();

        let initial = SettingsRepository::get_cleanup_config(&pool).await.unwrap();
        assert!(!initial.enabled);
        assert!(initial.prompt.is_empty());

        SettingsRepository::save_cleanup_config(
            &pool,
            &CleanupConfig {
                enabled: true,
                provider: "builtin-ai".into(),
                model: "qwen3.5:2b".into(),
                ollama_endpoint: None,
                prompt: "Prefer Elasticsearch.".into(),
            },
        )
        .await
        .unwrap();

        let summary = SettingsRepository::get_model_config(&pool).await.unwrap().unwrap();
        assert_eq!(summary.provider, "ollama");
        assert_eq!(summary.model, "llama3.2:latest");

        let saved = SettingsRepository::get_cleanup_config(&pool).await.unwrap();
        assert!(saved.enabled);
        assert_eq!(saved.provider, "builtin-ai");
        assert_eq!(saved.prompt, "Prefer Elasticsearch.");
    }

    #[tokio::test]
    async fn second_run_corrects_from_the_original_transcript() {
        let pool = migrated_pool().await;
        seed_meeting(
            &pool,
            "m1",
            &[
                SeedRow { id: "t1", start: Some(0.0), end: Some(1.0), speaker: Some("Ada"), text: "post gres" },
                SeedRow { id: "t2", start: Some(1.0), end: Some(2.0), speaker: Some("Ben"), text: "keep me" },
            ],
        )
        .await;

        snapshot_originals(&pool, "m1").await.unwrap();
        let lines = load_lines(&pool, "m1").await.unwrap();
        let parsed = parse_cleanup_output("[t1] Ada: Postgres\n");
        let entries = [entry("keep me", "kept")];
        let updates: Vec<(String, String)> = lines
            .iter()
            .filter_map(|line| {
                let next = choose_text(&line.source, parsed.get(&line.id).map(String::as_str), &entries);
                (next != line.current).then(|| (line.id.clone(), next))
            })
            .collect();
        write_changes(&pool, "m1", &updates).await.unwrap();

        let shown: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT id, transcript, transcript_original FROM transcripts WHERE meeting_id = 'm1' ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            shown,
            vec![
                ("t1".into(), "Postgres".into(), "post gres".into()),
                ("t2".into(), "kept".into(), "keep me".into()),
            ]
        );

        sqlx::query("UPDATE transcripts SET transcript = 'Postgres again' WHERE id = 't1'")
            .execute(&pool)
            .await
            .unwrap();
        let again = load_lines(&pool, "m1").await.unwrap();
        assert_eq!(again[0].source, "post gres");
        assert_eq!(again[0].current, "Postgres again");
        assert_eq!(again[1].source, "keep me");
    }
}
