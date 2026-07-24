// Wave 23 / PR-42-iii: per-segment LLM auto postprocess.
//
// Every ASR segment that is long enough to be worth rewriting gets an
// async pass through the user-configured LLM provider via
// summary::llm_client::generate_summary. The result is emitted to the
// frontend as `transcript-postprocessed`; failures go out as
// `transcript-postprocess-failed`. Segments below the length threshold
// skip the LLM call entirely so the stream is not flooded with
// trivial rewrites of "uh huh" / "yes" / "ok".

use crate::audio::post_processor;
use crate::database::repositories::setting::SettingsRepository;
use crate::summary::llm_client::{generate_summary, LLMError, LLMProvider};
use crate::summary::CustomOpenAIConfig;
use crate::llm_diagnostics::{LastTestResult, LLMDiagnosticsState};
use crate::llm_diagnostics::DiagnosticsSnapshot;
use once_cell::sync::OnceLock;
use serde::Serialize;
use sqlx::SqlitePool;
use std::str::FromStr;
use tauri::{AppHandle, Emitter, Manager};


/// Stable error codes for `correct_segment`. Frontend uses `code` to look up
/// localised text in `transcript.postprocess_error_<code>`; `message` is kept
/// for developer logs and as the i18n fallback.
#[derive(Debug, Clone, Serialize)]
pub struct PostprocessError {
    pub code: &'static str,
    pub message: String,
}

/// Stable identifiers for postprocess errors. Treat as a wire contract:
/// renaming a constant is a breaking change and must update CHANGELOG.
pub mod error_code {
    pub const PROVIDER_NOT_CONFIGURED: &str = "provider_not_configured";
    pub const UNSUPPORTED_PROVIDER: &str = "unsupported_provider";
    pub const CUSTOM_OPENAI_CONFIG_MISSING: &str = "custom_openai_config_missing";
    pub const API_KEY_MISSING: &str = "api_key_missing";
    pub const UPSTREAM_HTTP: &str = "upstream_http";
    pub const NETWORK: &str = "network";
    pub const UPSTREAM_EMPTY: &str = "upstream_empty";
    pub const CANCELLED: &str = "cancelled";
    pub const INTERNAL: &str = "internal";
    pub const AUTH_FAILED: &str = "auth_failed";
    pub const JSON_PARSE: &str = "json_parse";
    pub const UPSTREAM_RATE_LIMITED: &str = "upstream_rate_limited";
}

/// Map a typed `LLMError` to a `PostprocessError` with a stable code.
/// Replaces the string-prefix heuristic in `map_upstream_error`
/// (PR-42-iv-b) with a typed match.
fn map_llm_error(e: LLMError) -> PostprocessError {
    use LLMError::*;
    let message = e.to_string();
    let code = match &e {
        Cancelled => error_code::CANCELLED,
        Auth => error_code::AUTH_FAILED,
        ClientError { status: 429, .. } => error_code::UPSTREAM_RATE_LIMITED,
        ClientError { .. } => error_code::UPSTREAM_HTTP,
        ServerError { .. } => error_code::UPSTREAM_HTTP,
        Network(_) => error_code::NETWORK,
        JsonParse(_) => error_code::JSON_PARSE,
        Other(_) => error_code::INTERNAL,
    };
    PostprocessError { code, message }
}
/// Minimum CJK characters before a segment is worth LLM rewriting.
/// Shorter fragments tend to lose information when rewritten.
pub const MIN_CJK_CHARS: usize = 8;

/// Minimum ASCII alphanumeric characters before rewriting kicks in.
/// Roughly one short English sentence.
pub const MIN_ASCII_CHARS: usize = 20;

const SYSTEM_PROMPT: &str = "You are a transcript corrector. Read the ASR chunk and: \
- Fix obvious ASR errors (homophones, punctuation, spacing). \
- Preserve all proper nouns / project names / jargon (see <glossary>). \
- Do not translate. Do not summarize. Keep the language of the source. \
Output ONLY the corrected text, no commentary, no quotes.";

#[derive(Serialize, Clone)]
struct PostprocessedSegment {
    segment_id: String,
    text: String,
    latency_ms: u64,
}

#[derive(Serialize, Clone)]
struct PostprocessFailedSegment {
    segment_id: String,
    error: PostprocessError,
}

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
static APP: OnceLock<AppHandle> = OnceLock::new();

fn http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("Failed to build reqwest client for llm_postprocess")
    })
}

/// Wire the Tauri AppHandle once at startup. RecordingSaver has no
/// direct AppHandle parameter on add_transcript_segment, so the postprocess
/// path relies on this global.
pub fn init_app(app: AppHandle) {
    let _ = APP.set(app);
}

fn is_cjk(c: char) -> bool {
    let cp = c as u32;
    (0x4E00..=0x9FFF).contains(&cp) || (0x3400..=0x4DBF).contains(&cp)
}

pub fn should_skip_for_length(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }
    let cjk_count = trimmed.chars().filter(|c| is_cjk(*c)).count();
    if cjk_count > 0 {
        cjk_count < MIN_CJK_CHARS
    } else {
        let ascii_count = trimmed
            .chars()
            .filter(|c| c.is_ascii_alphabetic() || c.is_ascii_digit())
            .count();
        ascii_count < MIN_ASCII_CHARS
    }
}

fn build_glossary_block() -> Option<String> {
    let terms = post_processor::read_hotwords_for_llm();
    if terms.is_empty() {
        return None;
    }
    Some(format!("<glossary>\n{}\n</glossary>", terms.join("\n")))
}

fn build_user_prompt(text: &str) -> String {
    let terms = post_processor::read_hotwords_for_llm();
    render_user_prompt(SYSTEM_PROMPT, &terms, text)
}

/// Pure helper for prompt construction. Extracted so the prompt
/// format can be unit-tested without touching the global hotwords
/// state.
fn render_user_prompt(system_prompt: &str, glossary_terms: &[String], source: &str) -> String {
    let mut prompt = String::from(system_prompt);
    prompt.push_str("\n\n");
    if !glossary_terms.is_empty() {
        prompt.push_str(&format!("<glossary>\n{}\n</glossary>", glossary_terms.join("\n")));
        prompt.push_str("\n\n");
    }
    prompt.push_str(&format!("<source>\n{}\n</source>", source));
    prompt
}

async fn load_provider_inputs(
    pool: &SqlitePool,
) -> Result<
    (
        LLMProvider,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<u32>,
        Option<f32>,
        Option<f32>,
    ),
    String,
> {
    let setting = SettingsRepository::get_model_config(pool)
        .await
        .map_err(|e| PostprocessError { code: error_code::PROVIDER_NOT_CONFIGURED, message: format!("Failed to load provider config: {}", e) })?
        .ok_or_else(|| PostprocessError { code: error_code::PROVIDER_NOT_CONFIGURED, message: "LLM provider not configured".to_string() })?;
    let provider_str = setting.provider.clone();
    let provider = LLMProvider::from_str(&provider_str)
        .map_err(|_| PostprocessError { code: error_code::UNSUPPORTED_PROVIDER, message: format!("Unsupported LLM provider: {}", provider_str) })?;

    if provider_str == "custom-openai" {
        let cfg: CustomOpenAIConfig = setting
            .get_custom_openai_config()
            .ok_or_else(|| PostprocessError { code: error_code::CUSTOM_OPENAI_CONFIG_MISSING, message: "Custom OpenAI config missing endpoint or api_key".to_string() })?;
        Ok((
            provider,
            cfg.model,
            cfg.api_key.unwrap_or_default(),
            None,
            Some(cfg.endpoint),
            cfg.max_tokens.map(|v| v as u32),
            cfg.temperature,
            cfg.top_p,
        ))
    } else {
        let key = SettingsRepository::get_api_key(pool, &provider_str)
            .await
            .map_err(|e| PostprocessError { code: error_code::API_KEY_MISSING, message: format!("Failed to load api key for {}: {}", provider_str, e) })?
            .unwrap_or_default();
        Ok((
            provider,
            setting.model,
            key,
            setting.ollama_endpoint,
            None,
            None,
            None,
            None,
        ))
    }
}

pub async fn correct_segment(pool: &SqlitePool, text: &str) -> Result<String, PostprocessError> {
    let (provider, model_name, api_key, ollama_endpoint, custom_openai_endpoint, max_tokens, temperature, top_p) =
        load_provider_inputs(pool).await?;
    let user_prompt = build_user_prompt(text);
    match generate_summary(
        http_client(),
        &provider,
        &model_name,
        &api_key,
        "You are a transcript corrector. Fix obvious ASR errors. Preserve proper nouns. Keep source language. Output only the corrected text.",
        &user_prompt,
        ollama_endpoint.as_deref(),
        custom_openai_endpoint.as_deref(),
        max_tokens,
        temperature,
        top_p,
        None,
        None,
    )
    .await
    {
        Ok(text) => Ok(text),
        Err(e) => Err(map_llm_error(e)),
    }
}

pub fn spawn_segment_postprocess(segment_id: String, text: String) {
    if should_skip_for_length(&text) {
        return;
    }
    let app = match APP.get() {
        Some(a) => a.clone(),
        None => {
            log::warn!(
                "AppHandle not initialised; skipping postprocess for segment {}",
                segment_id
            );
            return;
        }
    };
    tauri::async_runtime::spawn(async move {
        let pool = match app.try_state::<crate::state::AppState>() {
            Some(state) => state.db_manager.pool().clone(),
            None => {
                log::warn!(
                    "AppState missing; skipping postprocess for segment {}",
                    segment_id
                );
                return;
            }
        };
        let start = std::time::Instant::now();
        match correct_segment(&pool, &text).await {
            Ok(corrected) => {
                let payload = PostprocessedSegment {
                    segment_id: segment_id.clone(),
                    text: corrected,
                    latency_ms: start.elapsed().as_millis() as u64,
                };
                if let Err(e) = app.emit("transcript-postprocessed", &payload) {
                    log::warn!("Failed to emit transcript-postprocessed: {}", e);
                }
            }
            Err(e) => {
                if let Some(diag) = app.try_state::<LLMDiagnosticsState>() {
                    diag.record_failure(e.code, &e.message);
                }
                let payload = PostprocessFailedSegment {
                    segment_id: segment_id.clone(),
                    error: e,
                };
                if let Err(emit_err) = app.emit("transcript-postprocess-failed", &payload) {
                    log::warn!("Failed to emit transcript-postprocess-failed: {}", emit_err);
                }
            }
        }
    });
}


/// Frontend "Test connection" probe: runs a minimal `generate_summary`
/// call against the configured provider, records latency + outcome into
/// the in-memory `LLMDiagnosticsState`, and emits `llm-diagnostics-updated`.
/// Failure paths do NOT push a `record_failure` entry — this is a manual
/// probe, not an in-meeting transcript failure.
#[tauri::command]
pub async fn test_llm_connection(
    app: tauri::AppHandle,
    diag: tauri::State<'_, LLMDiagnosticsState>,
) -> Result<LastTestResult, String> {
    let state = match app.try_state::<crate::state::AppState>() {
        Some(s) => s,
        None => return Err("AppState not initialized".into()),
    };
    let pool = state.db_manager.pool().clone();
    let inputs = match load_provider_inputs(&pool).await {
        Ok(v) => v,
        Err(e) => {
            let last = LastTestResult::failed(0, e.code, &e.message);
            diag.set_last_test(last.clone());
            let _ = app.emit(
                "llm-diagnostics-updated",
                &DiagnosticsSnapshot {
                    buckets: diag.buckets(),
                    last_test: diag.last_test(),
                },
            );
            return Ok(last);
        }
    };
    let (provider, model_name, api_key, ollama_endpoint, custom_openai_endpoint,
        max_tokens, temperature, top_p) = inputs;
    let start = std::time::Instant::now();
    let result = generate_summary(
        http_client(),
        &provider,
        &model_name,
        &api_key,
        "ping",
        "ping",
        ollama_endpoint.as_deref(),
        custom_openai_endpoint.as_deref(),
        Some(1).or(max_tokens),
        temperature,
        top_p,
        None,
        None,
    ).await;
    let latency_ms = start.elapsed().as_millis();
    let last = match result {
        Ok(_) => LastTestResult::ok(latency_ms),
        Err(e) => {
            let mapped = map_llm_error(e);
            LastTestResult::failed(latency_ms, mapped.code, &mapped.message)
        }
    };
    diag.set_last_test(last.clone());
    let _ = app.emit(
        "llm-diagnostics-updated",
        &DiagnosticsSnapshot {
            buckets: diag.buckets(),
            last_test: diag.last_test(),
        },
    );
    Ok(last)
}

/// Re-run postprocess for a single failed segment. Thin wrapper
/// around `spawn_segment_postprocess` so the frontend can call it
/// via `invoke` without exposing the spawn API directly.
#[tauri::command]
pub fn retry_segment_postprocess(segment_id: String, text: String) {
    spawn_segment_postprocess(segment_id, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_cjk_skipped() {
        assert!(should_skip_for_length("你好"));
        assert!(should_skip_for_length("今天"));
        assert!(!should_skip_for_length("你好世界这是一个完整的测试句子"));
    }

    #[test]
    fn short_ascii_skipped() {
        assert!(should_skip_for_length("Hello"));
        assert!(should_skip_for_length("OK"));
        assert!(!should_skip_for_length(
            "This is a long enough English sentence to trigger postprocess."
        ));
    }

    #[test]
    fn empty_skipped() {
        assert!(should_skip_for_length(""));
        assert!(should_skip_for_length("   "));
    }

    #[test]
    fn cjk_dominates_threshold_choice() {
        assert!(should_skip_for_length("OpenAI 发布 GPT"));
        assert!(!should_skip_for_length(
            "OpenAI 刚刚发布了全新的 GPT-5 模型,性能显著提升"
        ));
    }

    #[test]
    fn glossary_block_empty_when_no_hotwords() {
        post_processor::set_hotwords_for_llm(vec![]);
        assert!(build_glossary_block().is_none());
    }

    #[test]
    fn glossary_block_renders_terms() {
        post_processor::set_hotwords_for_llm(vec!["AGI".to_string(), "Meetily".to_string()]);
        let block = build_glossary_block().unwrap();
        assert!(block.contains("AGI"));
        assert!(block.contains("Meetily"));
        post_processor::set_hotwords_for_llm(vec![]);
    }
    // ---- is_cjk boundary tests ----
    #[test]
    fn is_cjk_basic_cjk_inside_range() {
        assert!(is_cjk('\u{4e2d}')); // U+4E00 CJK Unified Ideograph
        assert!(is_cjk('\u{9fff}')); // U+9FFF end of basic range
    }

    #[test]
    fn is_cjk_extension_a_inside_range() {
        assert!(is_cjk('\u{3400}')); // U+3400 CJK Extension A start
        assert!(is_cjk('\u{4dbf}')); // U+4DBF CJK Extension A end
    }

    #[test]
    fn is_cjk_hiragana_katakana_outside_range() {
        // Hiragana and Katakana are NOT in the CJK Unified Ideograph ranges.
        assert!(!is_cjk('\u{3042}')); // HIRAGANA LETTER A
        assert!(!is_cjk('\u{30a2}')); // KATAKANA LETTER A
    }

    #[test]
    fn is_cjk_ascii_punctuation_outside_range() {
        assert!(!is_cjk('a'));
        assert!(!is_cjk(' '));
        assert!(!is_cjk('\u{3002}')); // IDEOGRAPHIC FULL STOP
    }

    // ---- should_skip_for_length boundary tests ----
    #[test]
    fn threshold_cjk_exact_boundary_passes() {
        // Exactly MIN_CJK_CHARS (8) should NOT skip.
        let s = "\u{4e2d}\u{6587}\u{6d4b}\u{8bd5}\u{8bed}\u{53e5}\u{5185}\u{5bb9}";
        assert_eq!(s.chars().count(), MIN_CJK_CHARS);
        assert!(!should_skip_for_length(s));
    }

    #[test]
    fn threshold_cjk_one_below_boundary_skips() {
        let s = "\u{4e2d}\u{6587}\u{6d4b}\u{8bd5}\u{8bed}\u{53e5}\u{5185}"; // 7
        assert!(should_skip_for_length(s));
    }

    #[test]
    fn threshold_ascii_exact_boundary_passes() {
        let s: String = "a".repeat(MIN_ASCII_CHARS);
        assert!(!should_skip_for_length(&s));
    }

    #[test]
    fn threshold_ascii_one_below_boundary_skips() {
        let s: String = "a".repeat(MIN_ASCII_CHARS - 1);
        assert!(should_skip_for_length(&s));
    }

    #[test]
    fn threshold_mixed_cjk_dominates() {
        // When CJK chars are present at all, CJK threshold applies.
        let cjk = "\u{4e2d}\u{6587}\u{6d4b}\u{8bd5}\u{8bed}\u{53e5}\u{5185}\u{5bb9}\u{52a0}";
        let ascii = "a".repeat(50);
        assert!(!should_skip_for_length(&format!("{}{}", cjk, ascii)));
    }

    #[test]
    fn threshold_only_punctuation_skips() {
        // Pure punctuation: 0 CJK, 0 alphanumeric -> below both thresholds.
        assert!(should_skip_for_length("......,,,!!!"));
    }

    // ---- build_glossary_block ordering ----
    #[test]
    fn glossary_block_preserves_term_order() {
        post_processor::set_hotwords_for_llm(vec![
            "Charlie".to_string(),
            "Alpha".to_string(),
            "Bravo".to_string(),
        ]);
        let block = build_glossary_block().unwrap();
        let c = block.find("Charlie").unwrap();
        let a = block.find("Alpha").unwrap();
        let b = block.find("Bravo").unwrap();
        assert!(c < a && a < b, "glossary block must preserve insertion order");
        post_processor::set_hotwords_for_llm(vec![]);
    }

    // ---- render_user_prompt tests ----
    #[test]
    fn render_prompt_no_glossary() {
        let p = render_user_prompt("SYS", &[], "hello");
        assert!(p.starts_with("SYS\n\n"));
        assert!(p.ends_with("<source>\nhello\n</source>"));
        assert!(!p.contains("<glossary>"));
    }

    #[test]
    fn render_prompt_with_glossary_includes_terms() {
        let terms = vec!["Meetily".to_string(), "AGI".to_string()];
        let p = render_user_prompt("SYS", &terms, "hi");
        assert!(p.contains("<glossary>\nMeetily\nAGI\n</glossary>"));
        assert!(p.contains("<source>\nhi\n</source>"));
        let g = p.find("<glossary>").unwrap();
        let s = p.find("<source>").unwrap();
        assert!(g < s);
    }

    #[test]
    fn render_prompt_empty_glossary_omits_block() {
        // Same as no-glossary: empty slice must not produce a glossary block.
        let p = render_user_prompt("SYS", &[], "x");
        assert!(!p.contains("<glossary>"));
    }

    #[test]
    fn render_prompt_preserves_term_lines() {
        let terms = vec!["alpha".to_string(), "beta".to_string()];
        let p = render_user_prompt("SYS", &terms, "x");
        assert!(p.contains("alpha"));
        assert!(p.contains("beta"));
    }


    // ---- provider-side error construction (PR-42-iv-b, retained) ----

    #[test]
    fn error_provider_not_configured_carries_code() {
        let e = PostprocessError {
            code: error_code::PROVIDER_NOT_CONFIGURED,
            message: "LLM provider not configured".to_string(),
        };
        assert_eq!(e.code, "provider_not_configured");
        assert_eq!(e.message, "LLM provider not configured");
    }

    #[test]
    fn error_unsupported_provider_includes_name() {
        let e = PostprocessError {
            code: error_code::UNSUPPORTED_PROVIDER,
            message: "Unsupported LLM provider: nope".to_string(),
        };
        assert_eq!(e.code, "unsupported_provider");
        assert!(e.message.contains("nope"));
    }

    #[test]
    fn error_api_key_missing_includes_provider() {
        let e = PostprocessError {
            code: error_code::API_KEY_MISSING,
            message: "OpenAI API key is not configured".to_string(),
        };
        assert_eq!(e.code, "api_key_missing");
        assert!(e.message.contains("OpenAI"));
    }

    // ---- map_llm_error classification (PR-42-iv-c) ----

    #[test]
    fn error_auth_failed_carries_code() {
        let e = map_llm_error(LLMError::Auth);
        assert_eq!(e.code, error_code::AUTH_FAILED);
        assert!(e.message.contains("401") || e.message.contains("403"));
    }

    #[test]
    fn error_rate_limited_carries_code() {
        let e = map_llm_error(LLMError::ClientError {
            status: 429,
            body: "rate limit exceeded".to_string(),
        });
        assert_eq!(e.code, error_code::UPSTREAM_RATE_LIMITED);
    }

    #[test]
    fn error_client_error_carries_http_code() {
        let e = map_llm_error(LLMError::ClientError {
            status: 400,
            body: "bad request".to_string(),
        });
        assert_eq!(e.code, error_code::UPSTREAM_HTTP);
    }

    #[test]
    fn error_server_error_carries_status() {
        let e = map_llm_error(LLMError::ServerError {
            status: 503,
            body: "service unavailable".to_string(),
        });
        assert_eq!(e.code, error_code::UPSTREAM_HTTP);
    }

    #[test]
    fn error_json_parse_carries_code() {
        let e = map_llm_error(LLMError::JsonParse("unexpected token".to_string()));
        assert_eq!(e.code, error_code::JSON_PARSE);
        assert!(e.message.contains("parse"));
    }

    #[test]
    fn error_network_carries_code() {
        let e = map_llm_error(LLMError::Network("connection refused".to_string()));
        assert_eq!(e.code, error_code::NETWORK);
    }

    #[test]
    fn error_cancelled_carries_code() {
        let e = map_llm_error(LLMError::Cancelled);
        assert_eq!(e.code, error_code::CANCELLED);
    }

    #[test]
    fn error_other_carries_internal_code() {
        let e = map_llm_error(LLMError::Other("unexpected".to_string()));
        assert_eq!(e.code, error_code::INTERNAL);
    }
}
