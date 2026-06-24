//! Summary-model resolution for the meeting-log summary. Resolution order:
//!   1. a runtime override set from the Settings dropdown (if installed)
//!   2. env `SUMMARY_MODEL` (if installed)
//!   3. env `SUMMARY_MODEL_FALLBACK` (if installed)
//!   4. env `SUMMARY_MODEL` anyway (last resort)
//! Never panics — a failed `ollama list` just falls through to the primary.

use super::config::config;
use serde::Deserialize;
use std::sync::Mutex;

static SUMMARY_OVERRIDE: Mutex<Option<String>> = Mutex::new(None);

pub fn set_summary_override(model: Option<String>) {
    if let Ok(mut g) = SUMMARY_OVERRIDE.lock() {
        *g = model.filter(|m| !m.trim().is_empty());
    }
}

pub fn summary_override() -> Option<String> {
    SUMMARY_OVERRIDE.lock().ok().and_then(|g| g.clone())
}

#[derive(Deserialize)]
struct TagModel {
    name: String,
}

#[derive(Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<TagModel>,
}

/// List installed Ollama model names (e.g. "qwen3.5:9b").
pub async fn list_installed_models() -> Result<Vec<String>, String> {
    let cfg = config();
    let url = format!("{}/api/tags", cfg.ollama_url.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("ollama tags request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("ollama /api/tags returned {}", resp.status()));
    }
    let parsed: TagsResponse = resp
        .json()
        .await
        .map_err(|e| format!("ollama tags decode failed: {e}"))?;
    Ok(parsed.models.into_iter().map(|m| m.name).collect())
}

/// True if `target` matches an installed model name (exact, or same name with a
/// different/elided tag, so "qwen3.5:9b" matches "qwen3.5:9b" and a bare name
/// matches "<name>:latest").
fn is_installed(installed: &[String], target: &str) -> bool {
    let t = target.trim();
    if t.is_empty() {
        return false;
    }
    installed.iter().any(|m| {
        m == t
            || m == &format!("{t}:latest")
            || m.split(':').next() == t.split(':').next()
                && (t.contains(':') == m.contains(':') || !t.contains(':'))
    })
}

/// Resolve the summary model to actually call, applying the fallback chain.
pub async fn resolve_summary_model() -> String {
    let cfg = config();
    let primary = summary_override().unwrap_or_else(|| cfg.summary_model.clone());

    match list_installed_models().await {
        Ok(installed) => {
            if is_installed(&installed, &primary) {
                primary
            } else if is_installed(&installed, &cfg.summary_model_fallback) {
                log::warn!(
                    "📝 meeting-log: summary model '{}' not installed; falling back to '{}'",
                    primary,
                    cfg.summary_model_fallback
                );
                cfg.summary_model_fallback.clone()
            } else {
                log::warn!(
                    "📝 meeting-log: neither '{}' nor fallback '{}' installed; trying '{}' anyway",
                    primary,
                    cfg.summary_model_fallback,
                    primary
                );
                primary
            }
        }
        Err(e) => {
            log::warn!("📝 meeting-log: could not query ollama ({e}); using '{}'", primary);
            primary
        }
    }
}
