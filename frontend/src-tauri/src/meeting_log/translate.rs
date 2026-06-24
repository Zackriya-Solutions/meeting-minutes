//! Translation client (Feature 5). Sends Thai (or mixed) text to the sidecar's
//! `/translate` endpoint, which runs `translategemma` via Ollama with a
//! term-pinning prompt so technical terms stay in English. English-only input
//! is short-circuited here to avoid a needless round-trip.

use super::config::config;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct TranslateRequest {
    model: String,
    text: String,
    glossary: Vec<String>,
    target: String,
}

#[derive(Debug, Deserialize)]
struct TranslateResponse {
    #[serde(default)]
    translation: String,
}

/// True if the text contains any Thai characters.
fn has_thai(text: &str) -> bool {
    text.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c))
}

/// True if the text contains any ASCII letters (proxy for "has English words").
fn has_latin(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_alphabetic())
}

/// Should we skip translating this text for the given target? We skip when the
/// text is already entirely in the target language (nothing to translate).
fn already_target(text: &str, target: &str) -> bool {
    match target {
        "th" => !has_latin(text), // already pure Thai (no English words)
        _ => !has_thai(text),     // target en: already has no Thai
    }
}

/// Translate `text` into `target` ("th" or "en"), pinning technical terms.
/// Text already in the target language is returned unchanged.
pub async fn translate_to(text: &str, target: &str) -> Result<String, String> {
    let trimmed = text.trim();
    let target = if target == "th" { "th" } else { "en" };
    if trimmed.is_empty() || already_target(trimmed, target) {
        return Ok(trimmed.to_string());
    }

    let cfg = config();
    // translategemma is →EN only; use a Thai-capable model for →TH.
    let model = if target == "th" {
        cfg.translate_model_th.clone()
    } else {
        cfg.translate_model.clone()
    };
    let req = TranslateRequest {
        model,
        text: trimmed.to_string(),
        glossary: cfg.glossary.clone(),
        target: target.to_string(),
    };
    let url = format!("{}/translate", cfg.sidecar_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("translate request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("sidecar /translate returned {}", resp.status()));
    }
    let parsed: TranslateResponse = resp
        .json()
        .await
        .map_err(|e| format!("translate decode failed: {e}"))?;
    Ok(parsed.translation.trim().to_string())
}
