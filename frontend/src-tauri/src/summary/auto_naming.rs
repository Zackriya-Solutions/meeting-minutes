use crate::database::repositories::setting::SettingsRepository;
use crate::state::AppState;
use crate::summary::llm_client::{generate_summary, LLMProvider};
use log::{error as log_error, info as log_info};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};

/// Response from auto-naming a meeting
#[derive(Debug, Serialize, Deserialize)]
pub struct AutoNameResponse {
    pub meeting_id: String,
    pub title: String,
    pub success: bool,
}

/// Auto-generate a meeting title from transcript content using LLM.
///
/// This command:
/// 1. Fetches all transcript chunks for the meeting
/// 2. Sends them to the configured LLM with a title-generation prompt
/// 3. Saves the generated title to the database
///
/// # Arguments
/// * `meeting_id` - The meeting to generate a title for
///
/// # Returns
/// The generated title and success status
#[tauri::command]
pub async fn api_auto_generate_title<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<AutoNameResponse, String> {
    log_info!("Auto-generating title for meeting: {}", meeting_id);

    let pool = state.db_manager.pool();

    // 1. Fetch transcript text from transcript_chunks (full text used for summary generation)
    let chunk_text: Option<String> = sqlx::query_scalar(
        "SELECT transcript_text FROM transcript_chunks WHERE meeting_id = ? LIMIT 1",
    )
    .bind(&meeting_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to fetch transcript chunks: {}", e))?;

    // Fallback: fetch individual transcript segments
    let combined = if let Some(text) = chunk_text {
        text
    } else {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT transcript FROM transcripts WHERE meeting_id = ? ORDER BY timestamp ASC",
        )
        .bind(&meeting_id)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("Failed to fetch transcripts: {}", e))?;

        if rows.is_empty() {
            return Err("No transcript data available for auto-naming".to_string());
        }
        rows.into_iter().map(|(t,)| t).collect::<Vec<_>>().join("\n")
    };

    // Truncate to ~3000 chars for title generation
    let truncated = if combined.len() > 3000 {
        &combined[..3000]
    } else {
        &combined
    };

    // 2. Get model config
    let settings = SettingsRepository::get_model_config(pool)
        .await
        .map_err(|e| format!("Failed to get model config: {}", e))?
        .ok_or_else(|| "No model configuration found".to_string())?;

    let provider = LLMProvider::from_str(&settings.provider)?;
    let model = settings.model.clone();

    // Determine API key
    let api_key = match &provider {
        LLMProvider::Ollama | LLMProvider::BuiltInAI => String::new(),
        LLMProvider::CustomOpenAI => {
            let config = settings.get_custom_openai_config();
            config.and_then(|c| c.api_key).unwrap_or_default()
        }
        _ => SettingsRepository::get_api_key(pool, &settings.provider)
            .await
            .unwrap_or(None)
            .unwrap_or_default(),
    };

    // 3. Call LLM to generate title
    let system_prompt = r#"You are a meeting title generator. Given a meeting transcript, generate a concise, descriptive title (max 80 characters).

RULES:
1. The title must be in the SAME language as the transcript
2. Be specific — include the main topic, not just "Meeting Notes"
3. If it's a standup, include the date
4. If it's an interview, include the role being discussed
5. Output ONLY the title text, no quotes, no explanation

Examples:
- "Q3 Backend Planning — Payment Integration"
- "Daily Standup 2026-08-11"
- "Senior Developer Interview — Frontend Team"
- "Client Demo: Dashboard Redesign"
- "Họp-planning Q3 — Tích hợp thanh toán""#;

    let user_prompt = format!(
        "Generate a meeting title for this transcript:\n\n<transcript>\n{}\n</transcript>",
        truncated
    );

    let custom_endpoint = settings
        .get_custom_openai_config()
        .map(|c| c.endpoint);

    let response = match generate_summary(
        &reqwest::Client::new(),
        &provider,
        &model,
        &api_key,
        system_prompt,
        &user_prompt,
        settings.ollama_endpoint.as_deref(),
        custom_endpoint.as_deref(),
        None,    // max_tokens
        Some(0.3), // temperature — low for deterministic title
        None,    // top_p
        None,    // app_data_dir
        None,    // cancellation_token
    )
    .await
    {
        Ok(resp) => resp,
        Err(e) => {
            log_error!(
                "LLM title generation failed for {}: {}. Falling back to heuristic title.",
                meeting_id,
                e
            );
            return generate_fallback_title(pool, &meeting_id).await;
        }
    };

    // 4. Clean up the title (remove quotes, extra whitespace, markdown)
    let title = response
        .trim()
        .trim_matches(|c| c == '"' || c == '\'' || c == '`')
        .replace("# ", "")
        .replace("**", "")
        .replace("*", "")
        .lines()
        .next()
        .unwrap_or("Untitled Meeting")
        .to_string();

    // 5. Save to database
    sqlx::query("UPDATE meetings SET title = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(&title)
        .bind(&meeting_id)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to save title: {}", e))?;

    log_info!(
        "Auto-generated title for meeting {}: '{}'",
        meeting_id,
        title
    );

    Ok(AutoNameResponse {
        meeting_id,
        title,
        success: true,
    })
}

/// Extract transcript text for a meeting (chunk text first, then individual
/// segments). Shared by both the LLM path and the heuristic fallback.
async fn fetch_transcript_text(
    pool: &sqlx::SqlitePool,
    meeting_id: &str,
) -> Result<String, String> {
    let chunk_text: Option<String> = sqlx::query_scalar(
        "SELECT transcript_text FROM transcript_chunks WHERE meeting_id = ? LIMIT 1",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to fetch transcript chunks: {}", e))?;

    if let Some(text) = chunk_text {
        return Ok(text);
    }

    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT transcript FROM transcripts WHERE meeting_id = ? ORDER BY timestamp ASC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to fetch transcripts: {}", e))?;

    if rows.is_empty() {
        return Err("No transcript data available for auto-naming".to_string());
    }
    Ok(rows.into_iter().map(|(t,)| t).collect::<Vec<_>>().join("\n"))
}

/// Generate (and persist) a non-LLM heuristic title for a meeting.
/// Shared implementation used by `api_generate_title_fallback` and by the
/// automatic fallback inside `api_auto_generate_title` when the LLM call fails.
async fn generate_fallback_title(
    pool: &sqlx::SqlitePool,
    meeting_id: &str,
) -> Result<AutoNameResponse, String> {
    log_info!("Generating fallback (non-LLM) title for meeting: {}", meeting_id);

    let combined = fetch_transcript_text(pool, meeting_id).await?;
    let title = build_heuristic_title(&combined);

    sqlx::query("UPDATE meetings SET title = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(&title)
        .bind(meeting_id)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to save title: {}", e))?;

    log_info!("Fallback title for meeting {}: '{}'", meeting_id, title);

    Ok(AutoNameResponse {
        meeting_id: meeting_id.to_string(),
        title,
        success: true,
    })
}

/// Generate a fallback title from transcript content WITHOUT calling an LLM.
///
/// Used when `api_auto_generate_title` fails (no model configured, API error,
/// offline, etc.). The heuristic takes the most content-bearing sentence from
/// the transcript and truncates it to a readable length. The result is saved
/// to the database so the meeting never stays on a default timestamp title.
#[tauri::command]
pub async fn api_generate_title_fallback<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<AutoNameResponse, String> {
    generate_fallback_title(state.db_manager.pool(), &meeting_id).await
}

/// Pick the most representative sentence from a transcript and trim it to a
/// reasonable title length. Pure, deterministic, no network/LLM.
fn build_heuristic_title(transcript: &str) -> String {
    // Split into sentences, ignore very short or empty ones.
    let sentences: Vec<&str> = transcript
        .split(['.', '!', '?', '\n'])
        .map(|s| s.trim())
        .filter(|s| s.len() >= 12)
        .collect();

    let base = if let Some(longest) = sentences
        .iter()
        .max_by_key(|s| s.split_whitespace().count())
    {
        *longest
    } else {
        transcript.trim()
    };

    // Collapse whitespace and cap to 70 chars at a word boundary.
    let collapsed: String = base.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.len() <= 70 {
        if collapsed.is_empty() {
            "Untitled Meeting".to_string()
        } else {
            collapsed
        }
    } else {
        let mut cut = &collapsed[..70];
        // Extend to the next space so we don't split a word.
        if let Some(sp) = cut.rfind(' ') {
            cut = &cut[..sp];
        }
        format!("{}…", cut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heuristic_title_picks_longest_sentence() {
        let transcript = "Hi. Let's sync about the payment integration on staging tomorrow morning. Short bit. We also need to review the API contract.";
        let title = build_heuristic_title(transcript);
        // The longest sentence by word count is the payment integration one.
        assert!(title.contains("payment integration"));
        assert!(title.len() <= 70);
    }

    #[test]
    fn heuristic_title_collapses_whitespace_and_truncates() {
        let transcript = "This is an extremely long sentence that should be trimmed down to a reasonable title length because otherwise it would overflow the UI element and look broken when displayed in the sidebar of the application window";
        let title = build_heuristic_title(transcript);
        assert!(title.len() <= 71); // 70 + ellipsis
        assert!(title.ends_with('…'));
        assert!(!title.contains("application window"));
    }

    #[test]
    fn heuristic_title_handles_empty_input() {
        assert_eq!(build_heuristic_title(""), "Untitled Meeting");
        assert_eq!(build_heuristic_title("   "), "Untitled Meeting");
    }

    #[test]
    fn heuristic_title_handles_only_short_sentences() {
        // No sentence reaches the >=12 char filter, falls back to trimmed full text.
        let transcript = "ok.\nhi.\nyo.";
        let title = build_heuristic_title(transcript);
        assert_eq!(title, "ok. hi. yo.");
    }
}

/// Check if a meeting title is still the default timestamp format.
///
/// Default titles look like: "2026-08-11 14:30:00" or "Meeting 2026-08-11"
/// Returns true if the title should be auto-renamed.
#[tauri::command]
pub async fn api_should_auto_name(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<bool, String> {
    let title: Option<String> = sqlx::query_scalar("SELECT title FROM meetings WHERE id = ?")
        .bind(&meeting_id)
        .fetch_optional(&state.db_manager.pool)
        .await
        .map_err(|e| format!("Failed to get meeting: {}", e))?;

    let title = match title {
        Some(t) => t,
        None => return Ok(false),
    };

    // Check if title looks like a default timestamp
    let is_default = title.starts_with("Meeting ")
        || title.starts_with("meeting_")
        || title
            .chars()
            .all(|c| c.is_numeric() || c == '-' || c == ' ' || c == ':' || c == 'T')
        || title.len() < 5;

    Ok(is_default)
}

