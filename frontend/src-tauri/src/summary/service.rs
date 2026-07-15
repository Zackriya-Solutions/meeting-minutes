use crate::database::repositories::{
    meeting::MeetingsRepository, setting::SettingsRepository, summary::SummaryProcessesRepository,
};
use crate::ollama::metadata::ModelMetadataCache;
use crate::summary::language_detection::detect_summary_language;
use crate::summary::llm_client::LLMProvider;
use crate::summary::metadata::read_detected_summary_language_from_metadata;
use crate::summary::processor::{extract_meeting_name_from_markdown, generate_meeting_summary};
use crate::summary::templates::{self, Template};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

// Global cache for model metadata (5 minute TTL)
static METADATA_CACHE: Lazy<ModelMetadataCache> =
    Lazy::new(|| ModelMetadataCache::new(Duration::from_secs(300)));

// Global registry for cancellation tokens (thread-safe)
static CANCELLATION_REGISTRY: Lazy<Arc<Mutex<HashMap<String, CancellationToken>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

/// Strips the first `#` heading line; returns "" if no `#` is found.
fn strip_leading_title(markdown: &str) -> String {
    if let Some(hash_pos) = markdown.find('#') {
        let body_start = markdown[hash_pos..]
            .find('\n')
            .map_or(markdown.len(), |line_end| hash_pos + line_end);
        markdown[body_start..].trim_start().to_string()
    } else {
        String::new()
    }
}

/// Strips the leading H1 (`# Title\n...`) only when the markdown starts with one.
/// No-op on already-stripped values, values starting with `## Subheading`, or values
/// without any heading. Avoids the silent-empty-return case where `strip_leading_title`
/// returns "" for input lacking a leading `#`.
fn strip_title_if_present(markdown: &str) -> String {
    if markdown.trim_start().starts_with("# ") {
        strip_leading_title(markdown)
    } else {
        markdown.to_string()
    }
}

const GENERATION_METADATA_FIELD: &str = "summary_generation";
const SUMMARY_PIPELINE_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SummaryCacheSource {
    transcript_fingerprint: String,
    custom_prompt_fingerprint: String,
    template_id: String,
    template_fingerprint: String,
    token_threshold: usize,
    model_provider: String,
    model_name: String,
    ollama_endpoint: Option<String>,
    custom_openai_endpoint: Option<String>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SummaryGenerationMetadata {
    source: SummaryCacheSource,
    output_language: String,
    pipeline_version: u32,
}

fn stable_text_fingerprint(text: &str) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{:016x}:{}", hash, text.len())
}

#[allow(clippy::too_many_arguments)]
fn build_summary_cache_source(
    text: &str,
    custom_prompt: &str,
    template_id: &str,
    template_fingerprint: &str,
    token_threshold: usize,
    model_provider: &str,
    model_name: &str,
    ollama_endpoint: Option<&str>,
    custom_openai_endpoint: Option<&str>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
) -> SummaryCacheSource {
    SummaryCacheSource {
        transcript_fingerprint: stable_text_fingerprint(text),
        custom_prompt_fingerprint: stable_text_fingerprint(custom_prompt),
        template_id: template_id.to_string(),
        template_fingerprint: template_fingerprint.to_string(),
        token_threshold,
        model_provider: model_provider.to_string(),
        model_name: model_name.to_string(),
        ollama_endpoint: ollama_endpoint.map(str::to_string),
        custom_openai_endpoint: custom_openai_endpoint.map(str::to_string),
        max_tokens,
        temperature,
        top_p,
    }
}

pub(crate) fn template_cache_fingerprint(template: &Template) -> String {
    let mut rendered_template = format!(
        "pipeline={}\n{}\n---SECTION-INSTRUCTIONS---\n{}",
        template.pipeline.as_deref().unwrap_or("generic"),
        template.to_markdown_structure(),
        template.to_section_instructions()
    );
    if template.pipeline.as_deref() == Some("standup_v2") {
        rendered_template.push_str("\n---STANDUP-EXTRACTION-CONTRACT---\n");
        rendered_template
            .push_str(&crate::summary::standup::extraction_contract_fingerprint_material());
    }
    stable_text_fingerprint(&rendered_template)
}

fn build_summary_result_json(
    final_markdown: &str,
    source: SummaryCacheSource,
    output_language: &str,
    structured_result: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut result = serde_json::json!({
        "markdown": strip_title_if_present(final_markdown),
        GENERATION_METADATA_FIELD: SummaryGenerationMetadata {
            source,
            output_language: output_language.to_string(),
            pipeline_version: SUMMARY_PIPELINE_VERSION,
        },
    });
    if let Some(structured_result) = structured_result {
        result["standup_v2"] = structured_result;
    }
    result
}

/// Summary service - handles all summary generation logic
pub struct SummaryService;

impl SummaryService {
    /// Registers a new cancellation token for a meeting
    fn register_cancellation_token(meeting_id: &str) -> CancellationToken {
        let token = CancellationToken::new();
        if let Ok(mut registry) = CANCELLATION_REGISTRY.lock() {
            registry.insert(meeting_id.to_string(), token.clone());
            info!("Registered cancellation token for meeting: {}", meeting_id);
        }
        token
    }

    /// Cancels the summary generation for a meeting
    pub fn cancel_summary(meeting_id: &str) -> bool {
        if let Ok(registry) = CANCELLATION_REGISTRY.lock() {
            if let Some(token) = registry.get(meeting_id) {
                info!("Cancelling summary generation for meeting: {}", meeting_id);
                token.cancel();
                return true;
            }
        }
        warn!(
            "No active summary generation found for meeting: {}",
            meeting_id
        );
        false
    }

    /// Cleans up the cancellation token after processing completes
    fn cleanup_cancellation_token(meeting_id: &str) {
        if let Ok(mut registry) = CANCELLATION_REGISTRY.lock() {
            if registry.remove(meeting_id).is_some() {
                info!("Cleaned up cancellation token for meeting: {}", meeting_id);
            }
        }
    }

    async fn read_detected_summary_language(pool: &SqlitePool, meeting_id: &str) -> Option<String> {
        let meeting = match MeetingsRepository::get_meeting_metadata(pool, meeting_id).await {
            Ok(Some(meeting)) => meeting,
            Ok(None) => {
                warn!(
                    "Meeting not found while reading detected summary language: {}",
                    meeting_id
                );
                return None;
            }
            Err(e) => {
                warn!(
                    "Failed to read meeting metadata for detected summary language (meeting_id={}): {}",
                    meeting_id, e
                );
                return None;
            }
        };

        let Some(folder_path) = meeting.folder_path.filter(|p| !p.trim().is_empty()) else {
            return None;
        };

        match read_detected_summary_language_from_metadata(Path::new(&folder_path)) {
            Ok(language) => language,
            Err(e) => {
                warn!(
                    "Failed to read detected summary language metadata for meeting_id={}: {}",
                    meeting_id, e
                );
                None
            }
        }
    }

    fn detect_summary_language_from_text(text: &str) -> Option<String> {
        let transcript_texts = [text.to_string()];
        let detection = detect_summary_language(&transcript_texts);
        match &detection.language {
            Some(language) => {
                info!(
                    "Detected transcript summary language for normalization: {}",
                    language
                );
            }
            None => {
                info!(
                    "Transcript summary language unknown for normalization: {:?}",
                    detection.reason
                );
            }
        }
        detection.language
    }

    /// Processes transcript in the background and generates summary
    ///
    /// This function is designed to be spawned as an async task and does not block
    /// the main thread. It updates the database with progress and results.
    ///
    /// # Arguments
    /// * `_app` - Tauri app handle (for future use)
    /// * `pool` - SQLx connection pool
    /// * `meeting_id` - Unique identifier for the meeting
    /// * `text` - Full transcript text
    /// * `model_provider` - LLM provider name (e.g., "ollama", "openai")
    /// * `model_name` - Specific model (e.g., "gpt-4", "llama3.2:latest")
    /// * `custom_prompt` - Optional user-provided context
    /// * `template_id` - Template identifier (e.g., "daily_standup", "standard_meeting")
    pub async fn process_transcript_background<R: tauri::Runtime>(
        _app: AppHandle<R>,
        pool: SqlitePool,
        meeting_id: String,
        text: String,
        model_provider: String,
        model_name: String,
        custom_prompt: String,
        template_id: String,
        summary_language: Option<String>,
    ) {
        let start_time = Instant::now();
        info!(
            "Starting background processing for meeting_id: {}",
            meeting_id
        );

        // Register cancellation token for this meeting
        let cancellation_token = Self::register_cancellation_token(&meeting_id);

        // Parse provider
        let provider = match LLMProvider::from_str(&model_provider) {
            Ok(p) => p,
            Err(e) => {
                Self::update_process_failed(&pool, &meeting_id, &e).await;
                return;
            }
        };

        // Enforce the central privacy policy before reading credentials or constructing
        // a network client. BuiltInAI and Ollama remain available in local-only mode.
        if !matches!(&provider, LLMProvider::BuiltInAI | LLMProvider::Ollama) {
            if let Err(e) =
                crate::llm::ensure_outbound_allowed(&pool, crate::llm::Purpose::Summary).await
            {
                let err_msg = e.to_string();
                Self::update_process_failed(&pool, &meeting_id, &err_msg).await;
                return;
            }
        }

        // Resolve both credential and transport. Managed DeepSeek bootstrap returns a
        // server-selected base URL; keeping only its token breaks local/pilot gateways.
        let (api_key, deepseek_base_url) = if provider == LLMProvider::Ollama
            || provider == LLMProvider::BuiltInAI
            || provider == LLMProvider::CustomOpenAI
        {
            // These providers don't require API keys from the standard database column
            (String::new(), None)
        } else if provider == LLMProvider::GigaChat {
            // GigaChat credentials live in app_settings_kv (Settings → Providers), not the
            // settings table. Resolve them to a single Basic-auth key the client can use.
            match crate::llm::providers::resolve_gigachat_auth_key(&pool).await {
                Some(key) => (key, None),
                None => {
                    let err_msg =
                        "GigaChat is not configured. Add your credentials in Settings → Providers."
                            .to_string();
                    Self::update_process_failed(&pool, &meeting_id, &err_msg).await;
                    return;
                }
            }
        } else if provider == LLMProvider::DeepSeek {
            match crate::llm::providers::resolve_deepseek_transport(&pool).await {
                Some(transport) => {
                    info!("Using DeepSeek transport at {}", transport.base_url);
                    (transport.api_key, Some(transport.base_url))
                }
                None => {
                    let err_msg = "Managed DeepSeek gateway is unavailable.".to_string();
                    Self::update_process_failed(&pool, &meeting_id, &err_msg).await;
                    return;
                }
            }
        } else {
            match SettingsRepository::get_api_key(&pool, &model_provider).await {
                Ok(Some(key)) if !key.is_empty() => (key, None),
                Ok(None) | Ok(Some(_)) => {
                    let err_msg = format!("API key not found for {}", &model_provider);
                    Self::update_process_failed(&pool, &meeting_id, &err_msg).await;
                    return;
                }
                Err(e) => {
                    let err_msg =
                        format!("Failed to retrieve API key for {}: {}", &model_provider, e);
                    Self::update_process_failed(&pool, &meeting_id, &err_msg).await;
                    return;
                }
            }
        };

        // Get Ollama endpoint if provider is Ollama
        let ollama_endpoint = if provider == LLMProvider::Ollama {
            match SettingsRepository::get_model_config(&pool).await {
                Ok(Some(config)) => config.ollama_endpoint,
                Ok(None) => None,
                Err(e) => {
                    info!("Failed to retrieve Ollama endpoint: {}, using default", e);
                    None
                }
            }
        } else {
            None
        };

        // Get CustomOpenAI config if provider is CustomOpenAI
        let (
            custom_openai_endpoint,
            custom_openai_api_key,
            custom_openai_max_tokens,
            custom_openai_temperature,
            custom_openai_top_p,
        ) = if provider == LLMProvider::CustomOpenAI {
            match SettingsRepository::get_custom_openai_config(&pool).await {
                Ok(Some(config)) => {
                    info!("✓ Using custom OpenAI endpoint: {}", config.endpoint);
                    (
                        Some(config.endpoint),
                        config.api_key,
                        config.max_tokens.map(|t| t as u32),
                        config.temperature,
                        config.top_p,
                    )
                }
                Ok(None) => {
                    let err_msg = "Custom OpenAI provider selected but no configuration found";
                    Self::update_process_failed(&pool, &meeting_id, err_msg).await;
                    return;
                }
                Err(e) => {
                    let err_msg = format!("Failed to retrieve custom OpenAI config: {}", e);
                    Self::update_process_failed(&pool, &meeting_id, &err_msg).await;
                    return;
                }
            }
        } else {
            (None, None, None, None, None)
        };

        // For CustomOpenAI, use its API key (if any) instead of the empty string
        let final_api_key = if provider == LLMProvider::CustomOpenAI {
            custom_openai_api_key.unwrap_or_default()
        } else {
            api_key
        };

        // Dynamically fetch context size based on provider and model
        let token_threshold = if provider == LLMProvider::Ollama {
            match METADATA_CACHE
                .get_or_fetch(&model_name, ollama_endpoint.as_deref())
                .await
            {
                Ok(metadata) => {
                    // Reserve 300 tokens for prompt overhead
                    let optimal = metadata.context_size.saturating_sub(300);
                    info!(
                        "✓ Using dynamic context for {}: {} tokens (chunk size: {})",
                        model_name, metadata.context_size, optimal
                    );
                    optimal
                }
                Err(e) => {
                    warn!(
                        "Failed to fetch context for {}: {}. Using default 4000",
                        model_name, e
                    );
                    4000 // Fallback to safe default
                }
            }
        } else if provider == LLMProvider::BuiltInAI {
            // Get model's context size from registry
            use crate::summary::summary_engine::models;
            let model = models::get_model_by_name(&model_name)
                .ok_or_else(|| format!("Unknown model: {}", model_name));

            match model {
                Ok(model_def) => {
                    // Reserve 300 tokens for prompt overhead
                    let optimal = model_def.context_size.saturating_sub(300) as usize;
                    info!(
                        "✓ Using BuiltInAI context size: {} tokens (chunk size: {})",
                        model_def.context_size, optimal
                    );
                    optimal
                }
                Err(e) => {
                    warn!("{}, using default 2048", e);
                    1748 // 2048 - 300 for overhead
                }
            }
        } else if provider == LLMProvider::DeepSeek {
            // Keep a quality/latency boundary below the model's raw context limit. Very long
            // meetings are summarized losslessly in chunks and fail as a whole if any chunk
            // fails, instead of relying on a single oversized gateway request.
            60_000
        } else {
            100_000
        };

        // Get app data directory for BuiltInAI provider
        let app_data_dir = _app.path().app_data_dir().ok();

        if let Some(code) = &summary_language {
            info!("📝 Summary language preference: {}", code);
        }

        let detected_summary_language = Self::read_detected_summary_language(&pool, &meeting_id)
            .await
            .or_else(|| Self::detect_summary_language_from_text(&text));

        if let Some(code) = &detected_summary_language {
            info!("📝 Detected transcript summary language: {}", code);
        }

        let template = match templates::get_template(&template_id) {
            Ok(template) => template,
            Err(e) => {
                let err_msg = format!("Failed to load template '{}': {}", template_id, e);
                Self::update_process_failed(&pool, &meeting_id, &err_msg).await;
                return;
            }
        };
        let template_fingerprint = template_cache_fingerprint(&template);

        let cache_source = build_summary_cache_source(
            &text,
            &custom_prompt,
            &template_id,
            &template_fingerprint,
            token_threshold,
            &model_provider,
            &model_name,
            ollama_endpoint.as_deref(),
            custom_openai_endpoint.as_deref(),
            custom_openai_max_tokens,
            custom_openai_temperature,
            custom_openai_top_p,
        );

        let client = reqwest::Client::new();
        let result = generate_meeting_summary(
            &client,
            &provider,
            &model_name,
            &final_api_key,
            &meeting_id,
            &text,
            &custom_prompt,
            &template_id,
            &template,
            token_threshold,
            ollama_endpoint.as_deref(),
            custom_openai_endpoint.as_deref(),
            deepseek_base_url.as_deref(),
            custom_openai_max_tokens,
            custom_openai_temperature,
            custom_openai_top_p,
            app_data_dir.as_ref(),
            Some(&cancellation_token),
            summary_language.as_deref(),
            detected_summary_language.as_deref(),
        )
        .await;

        let duration = start_time.elapsed().as_secs_f64();

        // Clean up cancellation token regardless of outcome
        Self::cleanup_cancellation_token(&meeting_id);

        match result {
            Ok((final_markdown, output_language, num_chunks, structured_result)) => {
                info!(
                    "✓ Successfully processed {} chunks for meeting_id: {}. Duration: {:.2}s",
                    num_chunks, meeting_id, duration
                );
                info!("Final markdown generated ({} chars)", final_markdown.len());

                // Standup V2 uses a stable renderer heading (`# Standup`) and keeps the
                // meaningful source/series title. Generic summaries may still propose a
                // descriptive H1 and retain the existing rename behavior.
                if structured_result.is_none() {
                    if let Some(name) = extract_meeting_name_from_markdown(&final_markdown)
                        .filter(|n| !n.is_empty())
                    {
                        info!("Extracted meeting name from summary: '{}'", name);
                        if let Err(e) =
                            MeetingsRepository::update_meeting_name(&pool, &meeting_id, &name).await
                        {
                            error!("Failed to update meeting name for {}: {}", meeting_id, e);
                        } else {
                            info!("Successfully updated meeting name for {}", meeting_id);
                        }
                    }
                }

                let standup_report = structured_result.as_ref().and_then(|value| {
                    match serde_json::from_value::<crate::summary::standup::StandupReport>(
                        value.clone(),
                    ) {
                        Ok(report) => Some(report),
                        Err(error) => {
                            error!("Failed to decode Standup V2 result for review sync: {error}");
                            None
                        }
                    }
                });
                let result_json = build_summary_result_json(
                    &final_markdown,
                    cache_source,
                    &output_language,
                    structured_result,
                );

                // Review records must be visible before the completed status wakes the UI.
                if let Some(report) = standup_report.as_ref() {
                    match crate::summary::standup_workflow::sync_standup_records(
                        &pool,
                        &meeting_id,
                        report,
                    )
                    .await
                    {
                        Ok(count) => info!(
                            "Synced {} pending Standup V2 review records for meeting_id: {}",
                            count, meeting_id
                        ),
                        Err(error) => {
                            let message =
                                format!("Failed to sync Standup V2 review records: {error}");
                            Self::update_process_failed(&pool, &meeting_id, &message).await;
                            return;
                        }
                    }
                }

                // Publish completed only after all user-visible persisted state is ready.
                if let Err(e) = SummaryProcessesRepository::update_process_completed(
                    &pool,
                    &meeting_id,
                    result_json,
                    num_chunks,
                    duration,
                )
                .await
                {
                    error!("Failed to save completed process for {}: {}", meeting_id, e);
                } else {
                    info!("Summary saved successfully for meeting_id: {}", meeting_id);
                }
            }
            Err(e) => {
                // Check if error is due to cancellation
                if e.contains("cancelled") {
                    info!(
                        "Summary generation was cancelled for meeting_id: {}",
                        meeting_id
                    );
                    if let Err(db_err) =
                        SummaryProcessesRepository::update_process_cancelled(&pool, &meeting_id)
                            .await
                    {
                        error!(
                            "Failed to update DB status to cancelled for {}: {}",
                            meeting_id, db_err
                        );
                    }
                } else {
                    Self::update_process_failed(&pool, &meeting_id, &e).await;
                }
            }
        }
    }

    /// Updates the summary process status to failed with error message
    ///
    /// # Arguments
    /// * `pool` - SQLx connection pool
    /// * `meeting_id` - Meeting identifier
    /// * `error_msg` - Error message to store
    async fn update_process_failed(pool: &SqlitePool, meeting_id: &str, error_msg: &str) {
        error!(
            "Processing failed for meeting_id {}: {}",
            meeting_id, error_msg
        );
        if let Err(e) =
            SummaryProcessesRepository::update_process_failed(pool, meeting_id, error_msg).await
        {
            error!(
                "Failed to update DB status to failed for {}: {}",
                meeting_id, e
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_leading_title_with_body() {
        let input = "# Meeting Title\nThis is the body.\nMore content.";
        let result = strip_leading_title(input);
        assert_eq!(result, "This is the body.\nMore content.");
    }

    #[test]
    fn test_strip_leading_title_only() {
        let input = "# Meeting Title";
        let result = strip_leading_title(input);
        assert_eq!(result, "");
    }

    #[test]
    fn test_strip_leading_title_no_heading() {
        let input = "No heading here.\nJust body.";
        let result = strip_leading_title(input);
        assert_eq!(result, "");
    }

    #[test]
    fn test_strip_leading_title_multiline_body() {
        let input = "# Title\n## Subheading\nParagraph 1\n\nParagraph 2";
        let result = strip_leading_title(input);
        assert_eq!(result, "## Subheading\nParagraph 1\n\nParagraph 2");
    }

    #[test]
    fn test_strip_leading_title_empty_after_heading() {
        let input = "# Title\n";
        let result = strip_leading_title(input);
        assert_eq!(result, "");
    }

    #[test]
    fn test_strip_leading_title_whitespace_after_heading() {
        let input = "# Title\n   \n Body with leading spaces";
        let result = strip_leading_title(input);
        assert_eq!(result, "Body with leading spaces");
    }

    #[test]
    fn test_strip_title_if_present_preserves_already_stripped() {
        assert_eq!(
            strip_title_if_present("## Action Items\nfoo"),
            "## Action Items\nfoo"
        );
    }

    #[test]
    fn test_strip_title_if_present_strips_leading_h1() {
        assert_eq!(
            strip_title_if_present("# Meeting Title\n## Action Items\nfoo"),
            "## Action Items\nfoo"
        );
    }

    #[test]
    fn test_strip_title_if_present_no_heading_preserved() {
        // Distinct from strip_leading_title which returns "" — this preserves input.
        assert_eq!(strip_title_if_present("Just body text"), "Just body text");
    }

    #[test]
    fn test_strip_title_if_present_hash_no_space_preserved() {
        // `#NoSpace` is not a markdown H1 — preserve.
        assert_eq!(strip_title_if_present("#NoSpace\nbody"), "#NoSpace\nbody");
    }

    #[test]
    fn test_strip_title_if_present_mid_document_h1_preserved() {
        // H1 after body content must NOT be stripped — guards the asymmetry where
        // extract_meeting_name_from_markdown scans every line for "# ".
        let input = "Some paragraph\n\n# H1 on line 3\n## Section\nbody";
        assert_eq!(strip_title_if_present(input), input);
    }

    #[test]
    fn test_strip_title_if_present_leading_whitespace_h1_stripped() {
        assert_eq!(
            strip_title_if_present("  # Title\n## Section\nbody"),
            "## Section\nbody"
        );
    }

    fn sample_cache_source() -> SummaryCacheSource {
        let template_fingerprint = stable_text_fingerprint("standard template prompt");
        build_summary_cache_source(
            "transcript body",
            "custom prompt",
            "standard_meeting",
            &template_fingerprint,
            3700,
            "ollama",
            "gemma3:1b",
            Some("http://localhost:11434"),
            None,
            None,
            None,
            None,
        )
    }

    fn test_template(section_title: &str) -> Template {
        Template {
            name: "Test".to_string(),
            description: "Test template".to_string(),
            pipeline: None,
            sections: vec![crate::summary::templates::TemplateSection {
                title: section_title.to_string(),
                instruction: "Summarize this section".to_string(),
                format: "paragraph".to_string(),
                item_format: None,
                example_item_format: None,
            }],
        }
    }

    #[test]
    fn test_template_cache_fingerprint_changes_with_rendered_template() {
        assert_ne!(
            template_cache_fingerprint(&test_template("Summary")),
            template_cache_fingerprint(&test_template("Decisions"))
        );
    }

    #[test]
    fn test_template_cache_fingerprint_changes_with_pipeline() {
        let generic = test_template("Summary");
        let mut standup = generic.clone();
        standup.pipeline = Some("standup_v2".to_string());
        assert_ne!(
            template_cache_fingerprint(&generic),
            template_cache_fingerprint(&standup)
        );
    }

    #[test]
    fn test_standup_template_fingerprint_includes_extraction_contract() {
        let mut standup = test_template("Summary");
        standup.pipeline = Some("standup_v2".to_string());
        let template_only = format!(
            "pipeline={}\n{}\n---SECTION-INSTRUCTIONS---\n{}",
            standup.pipeline.as_deref().unwrap_or("generic"),
            standup.to_markdown_structure(),
            standup.to_section_instructions()
        );

        assert_ne!(
            template_cache_fingerprint(&standup),
            stable_text_fingerprint(&template_only)
        );
    }

    #[test]
    fn test_result_json_strips_title_and_records_pipeline_metadata() {
        let result = build_summary_result_json(
            "# Встреча\n## Решения\nГотово",
            sample_cache_source(),
            "Russian",
            None,
        );

        assert_eq!(result["markdown"], "## Решения\nГотово");
        assert_eq!(result["summary_generation"]["output_language"], "Russian");
        assert_eq!(result["summary_generation"]["pipeline_version"], 2);
        assert!(result.get("english_cache").is_none());
    }

    #[test]
    fn test_generation_metadata_keeps_source_fingerprint() {
        let source = sample_cache_source();
        let expected = source.transcript_fingerprint.clone();
        let result = build_summary_result_json("# Title\nBody", source, "English", None);

        assert_eq!(
            result["summary_generation"]["source"]["transcript_fingerprint"],
            expected
        );
    }

    #[test]
    fn test_structured_standup_result_is_preserved() {
        let result = build_summary_result_json(
            "# Standup\nBody",
            sample_cache_source(),
            "English",
            Some(serde_json::json!({
                "schema_version": "standup_v2",
                "action_items": []
            })),
        );
        assert_eq!(result["standup_v2"]["schema_version"], "standup_v2");
    }
}
