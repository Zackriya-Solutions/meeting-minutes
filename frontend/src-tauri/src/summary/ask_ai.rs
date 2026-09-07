use crate::database::repositories::{
    meeting::MeetingsRepository,
    setting::SettingsRepository,
    summary::SummaryProcessesRepository,
};
use crate::state::AppState;
use crate::summary::llm_client::{generate_summary, LLMProvider};
use crate::summary::processor::clean_llm_markdown_output;
use log::{error as log_error, info as log_info, warn as log_warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{command, AppHandle, Manager, Runtime};

#[derive(Debug, Deserialize)]
pub struct ChatTurn {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct AskMeetingsAIResponse {
    pub answer: String,
    pub provider: String,
    pub model: String,
}

static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

fn get_http_client() -> &'static Client {
    HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(Duration::from_secs(180))
            .build()
            .unwrap_or_default()
    })
}

#[command]
pub async fn api_ask_meetings_ai<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    query: String,
    meeting_id: Option<String>,
    history: Option<Vec<ChatTurn>>,
) -> Result<AskMeetingsAIResponse, String> {
    log_info!(
        "api_ask_meetings_ai called: query='{}', meeting_id={:?}",
        query,
        meeting_id
    );

    let trimmed_query = query.trim();
    if trimmed_query.is_empty() {
        return Err("Query cannot be empty".to_string());
    }

    let pool = state.db_manager.pool();

    // 1. Fetch current AI configuration
    let config = match SettingsRepository::get_model_config(pool).await {
        Ok(Some(cfg)) => cfg,
        Ok(None) => {
            return Err("No AI model configured. Please go to Settings > Summary to select an AI provider and model.".to_string());
        }
        Err(e) => {
            log_error!("Failed to get model config: {}", e);
            return Err(format!("Database error retrieving model config: {}", e));
        }
    };

    let provider = LLMProvider::from_str(config.provider.trim()).map_err(|e| {
        log_error!("Invalid provider {}: {}", config.provider, e);
        e
    })?;

    let mut api_key = match SettingsRepository::get_api_key(pool, &config.provider.trim().to_lowercase()).await {
        Ok(key) => key.unwrap_or_default(),
        Err(e) => {
            log_warn!("Could not get API key for {}: {}", config.provider, e);
            String::new()
        }
    };

    let mut custom_openai_endpoint = None;
    let mut custom_max_tokens = None;
    let mut custom_temperature = None;
    let mut custom_top_p = None;

    if provider == LLMProvider::CustomOpenAI {
        if let Ok(Some(custom_cfg)) = SettingsRepository::get_custom_openai_config(pool).await {
            custom_openai_endpoint = Some(custom_cfg.endpoint);
            custom_max_tokens = custom_cfg.max_tokens.map(|t| t as u32);
            custom_temperature = custom_cfg.temperature;
            custom_top_p = custom_cfg.top_p;
            if let Some(k) = custom_cfg.api_key {
                if !k.trim().is_empty() {
                    api_key = k;
                }
            }
        } else {
            return Err("Custom OpenAI provider is selected, but no endpoint is configured. Please open Settings > Summary to configure it.".to_string());
        }
    }

    // Verify API key for cloud providers
    if matches!(provider, LLMProvider::OpenAI | LLMProvider::Claude | LLMProvider::Groq | LLMProvider::OpenRouter) && api_key.trim().is_empty() {
        let provider_name = match provider {
            LLMProvider::OpenAI => "OpenAI",
            LLMProvider::Claude => "Claude",
            LLMProvider::Groq => "Groq",
            LLMProvider::OpenRouter => "OpenRouter",
            _ => "the selected provider",
        };
        return Err(format!(
            "API key for {} is missing. Please go to Settings > Summary, enter your API key, and click Save.",
            provider_name
        ));
    }

    let app_data_dir: Option<PathBuf> = app.path().app_data_dir().ok();

    let mut effective_model = config.model.clone();

    // Verify Built-in AI model availability and integrity
    if provider == LLMProvider::BuiltInAI {
        use crate::summary::summary_engine::models;
        let mut model_is_usable = false;

        if let Some(model_def) = models::get_model_by_name(&effective_model) {
            if let Some(ref dir) = app_data_dir {
                let model_path = dir.join("models").join("summary").join(&model_def.gguf_file);
                if model_path.exists() {
                    if let Ok(meta) = std::fs::metadata(&model_path) {
                        let file_size_mb = meta.len() / (1024 * 1024);
                        let min_expected = (model_def.size_mb as f64 * 0.85) as u64;
                        if file_size_mb >= min_expected {
                            model_is_usable = true;
                        }
                    }
                }
            }
        }

        // If configured model is missing or corrupted, seamlessly fallback to an available intact model
        if !model_is_usable {
            if let Some(ref dir) = app_data_dir {
                for candidate in models::get_available_models() {
                    let candidate_path = dir.join("models").join("summary").join(&candidate.gguf_file);
                    if candidate_path.exists() {
                        if let Ok(meta) = std::fs::metadata(&candidate_path) {
                            let file_size_mb = meta.len() / (1024 * 1024);
                            let min_expected = (candidate.size_mb as f64 * 0.85) as u64;
                            if file_size_mb >= min_expected {
                                log_info!(
                                    "Configured model '{}' is corrupted or not ready. Auto-switching to available model '{}'.",
                                    effective_model, candidate.name
                                );
                                effective_model = candidate.name.clone();
                                model_is_usable = true;
                                // Auto-heal config in database
                                let _ = SettingsRepository::save_model_config(
                                    pool,
                                    &config.provider,
                                    &effective_model,
                                    &config.whisper_model,
                                    config.ollama_endpoint.as_deref(),
                                ).await;
                                break;
                            }
                        }
                    }
                }
            }
        }

        if !model_is_usable {
            return Err(
                "No Built-in AI model is downloaded yet. Please open Settings > Summary and download a model (such as Qwen 3.5 4B or Gemma 3 1B).".to_string()
            );
        }
    }

    // 2. Build Context based on selected scope
    let is_all_scope = meeting_id.as_deref().map_or(true, |id| id == "all" || id.trim().is_empty());

    let meeting_context = if is_all_scope {
        // Gather summaries and recent transcripts across all meetings
        let all_meetings = MeetingsRepository::get_meetings(pool)
            .await
            .map_err(|e| format!("Failed to retrieve meetings list: {}", e))?;

        if all_meetings.is_empty() {
            return Ok(AskMeetingsAIResponse {
                answer: "You don't have any meeting notes recorded in Meetily yet!\n\nTo start using **Ask Your Meetings AI**:\n1. Click **Start Recording** on the Home screen to capture and transcribe a meeting.\n2. Or click **Import Audio** to transcribe an existing audio recording.\n\nOnce a meeting is saved, you can ask anything about meeting duration, key decisions, topics, and action items!".to_string(),
                provider: config.provider,
                model: config.model,
            });
        }

        let mut context_lines = Vec::new();
            context_lines.push(format!("Total meetings available: {}", all_meetings.len()));
            context_lines.push("Summary overview of available meetings:".to_string());

            for (idx, meeting) in all_meetings.iter().take(15).enumerate() {
                let mut meeting_block = format!(
                    "\n=== Meeting #{} ===\nTitle: {}\nMeeting ID: {}\nDate: {}\n",
                    idx + 1,
                    meeting.title,
                    meeting.id,
                    meeting.created_at.0.to_rfc3339()
                );

                // Try fetching summary
                if let Ok(Some(proc)) = SummaryProcessesRepository::get_summary_data(pool, &meeting.id).await {
                    if let Some(res_str) = proc.result {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&res_str) {
                            if let Some(md) = v.get("markdown").and_then(|m| m.as_str()) {
                                let truncated = if md.len() > 1500 {
                                    format!("{}...\n[Summary truncated]", &md[..1500])
                                } else {
                                    md.to_string()
                                };
                                meeting_block.push_str(&format!("Summary:\n{}\n", truncated));
                            }
                        }
                    }
                }

                // If no summary, fetch a few transcript lines
                if let Ok((transcripts, count)) = MeetingsRepository::get_meeting_transcripts_paginated(pool, &meeting.id, 5, 0).await {
                    if !transcripts.is_empty() {
                        meeting_block.push_str(&format!("Key Transcript Samples (out of {} segments):\n", count));
                        for t in transcripts.iter().take(5) {
                            meeting_block.push_str(&format!("- [{}] {}\n", t.timestamp, t.transcript));
                        }
                    }
                }

                context_lines.push(meeting_block);
            }
            context_lines.join("\n")
    } else {
        // Targeted meeting
        let target_id = meeting_id.as_deref().unwrap();
        let meeting_details = MeetingsRepository::get_meeting(pool, target_id)
            .await
            .map_err(|e| format!("Failed to retrieve meeting details: {}", e))?
            .ok_or_else(|| format!("Meeting with ID '{}' not found", target_id))?;

        // Calculate duration accurately
        let mut min_audio_start: Option<f64> = None;
        let mut max_audio_end: Option<f64> = None;
        let mut sum_duration: f64 = 0.0;

        for t in &meeting_details.transcripts {
            if let Some(s) = t.audio_start_time {
                min_audio_start = Some(min_audio_start.map_or(s, |curr| curr.min(s)));
            }
            if let Some(e) = t.audio_end_time {
                max_audio_end = Some(max_audio_end.map_or(e, |curr| curr.max(e)));
            }
            if let Some(d) = t.duration {
                sum_duration += d;
            }
        }

        let calculated_seconds = match (min_audio_start, max_audio_end) {
            (Some(s), Some(e)) if e >= s => e - s,
            _ => sum_duration,
        };

        let duration_formatted = if calculated_seconds > 0.0 {
            let mins = (calculated_seconds / 60.0).floor() as u64;
            let secs = (calculated_seconds % 60.0).round() as u64;
            format!("{} minutes and {} seconds ({:02}:{:02})", mins, secs, mins, secs)
        } else {
            "Not available".to_string()
        };

        // Try getting existing summary
        let mut summary_text = String::new();
        if let Ok(Some(proc)) = SummaryProcessesRepository::get_summary_data(pool, target_id).await {
            if let Some(res_str) = proc.result {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&res_str) {
                    if let Some(md) = v.get("markdown").and_then(|m| m.as_str()) {
                        summary_text = md.to_string();
                    }
                }
            }
        }

        let mut transcript_lines = Vec::new();
        let mut char_count = 0;
        let char_limit = match provider {
            LLMProvider::BuiltInAI => 16000,
            LLMProvider::Ollama => 20000,
            _ => 40000,
        };
        for t in &meeting_details.transcripts {
            if char_count > char_limit {
                transcript_lines.push("... [Remaining transcript truncated to fit model context window] ...".to_string());
                break;
            }
            let line = format!("[{}] {}", t.timestamp, t.text);
            char_count += line.len();
            transcript_lines.push(line);
        }

        format!(
            "Meeting Title: {}\nMeeting ID: {}\nCreated At: {}\nTotal Duration: {}\n\nExisting Summary / Notes:\n{}\n\nMeeting Transcripts ({} segments):\n{}",
            meeting_details.title,
            meeting_details.id,
            meeting_details.created_at,
            duration_formatted,
            if summary_text.is_empty() { "No summary generated yet." } else { &summary_text },
            meeting_details.transcripts.len(),
            transcript_lines.join("\n")
        )
    };

    // 3. Assemble system prompt
    let system_prompt = "\
You are Meetily AI, an intelligent, privacy-first meeting assistant embedded directly in the Meetily desktop app.
Your task is to answer questions conversationally, accurately, and helpfully using the provided meeting context.

Guidelines:
- If the user asks about the meeting length, duration, or time taken (e.g. 'time taken by this meeting'), state the exact duration clearly (for example: 'Based on the meeting notes, the meeting duration was **5 minutes and 59 seconds** (05:59).').
- Cite specific meeting titles, participant statements, action items, or timestamps where appropriate.
- Format your response cleanly using GitHub-flavored Markdown (bolding, lists, code blocks).
- Be concise and direct. Do not add unnecessary fluff.
- If the answer cannot be found in the provided context, politely inform the user.";

    // 4. Assemble user prompt with history
    let mut full_user_prompt = String::new();
    full_user_prompt.push_str("<meeting_context>\n");
    full_user_prompt.push_str(&meeting_context);
    full_user_prompt.push_str("\n</meeting_context>\n\n");

    if let Some(turns) = history {
        if !turns.is_empty() {
            full_user_prompt.push_str("<conversation_history>\n");
            for turn in turns.iter().rev().take(6).rev() {
                let role_label = if turn.role.to_lowercase() == "user" { "User" } else { "AI Assistant" };
                full_user_prompt.push_str(&format!("{}: {}\n", role_label, turn.content));
            }
            full_user_prompt.push_str("</conversation_history>\n\n");
        }
    }

    full_user_prompt.push_str(&format!("User Question: {}", trimmed_query));

    // 5. Execute LLM call
    let client = get_http_client();
    let raw_answer = generate_summary(
        client,
        &provider,
        &effective_model,
        &api_key,
        system_prompt,
        &full_user_prompt,
        config.ollama_endpoint.as_deref(),
        custom_openai_endpoint.as_deref(),
        custom_max_tokens,
        custom_temperature,
        custom_top_p,
        app_data_dir.as_ref(),
        None,
    )
    .await
    .map_err(|e| {
        log_error!("Failed to generate AI response: {}", e);
        if provider == LLMProvider::Ollama {
            let host = config.ollama_endpoint.as_deref().unwrap_or("http://localhost:11434");
            format!("Failed to connect to Ollama at {}. Please make sure Ollama is running ('ollama serve') and model '{}' is installed. Error: {}", host, effective_model, e)
        } else {
            format!("Failed to get response from AI model ({} • {}): {}", config.provider, effective_model, e)
        }
    })?;

    let cleaned_answer = clean_llm_markdown_output(&raw_answer);

    Ok(AskMeetingsAIResponse {
        answer: cleaned_answer,
        provider: config.provider,
        model: effective_model,
    })
}

