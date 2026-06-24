//! Tauri command surface for the meeting-log feature set.

use super::config::config;
use super::memory::{self, SearchResult};
use super::translate;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct MeetingLogConfigView {
    pub log_root: String,
    pub vector_db_path: String,
    pub sidecar_url: String,
    pub translate_model: String,
    pub summary_model: String,
    pub embed_model: String,
    pub glossary: Vec<String>,
}

/// Expose resolved config (paths, models, glossary) to the frontend.
#[tauri::command]
pub fn meeting_log_config() -> MeetingLogConfigView {
    let c = config();
    MeetingLogConfigView {
        log_root: c.log_root.to_string_lossy().to_string(),
        vector_db_path: c.vector_db_path.to_string_lossy().to_string(),
        sidecar_url: c.sidecar_url.clone(),
        translate_model: c.translate_model.clone(),
        summary_model: c.summary_model.clone(),
        embed_model: c.embed_model.clone(),
        glossary: c.glossary.clone(),
    }
}

/// Translate one transcript segment into `target` ("th"/"en"), terms pinned.
/// Defaults to the configured `TRANSLATE_DEFAULT_TARGET` when target is omitted.
#[tauri::command]
pub async fn meeting_log_translate(
    text: String,
    target: Option<String>,
) -> Result<String, String> {
    let target = target.unwrap_or_else(|| config().translate_default_target.clone());
    translate::translate_to(&text, &target).await
}

/// Hybrid (dense + sparse) search over the local memory store.
#[tauri::command]
pub async fn meeting_log_search(
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SearchResult>, String> {
    memory::search(&query, limit.unwrap_or(10)).await
}

/// List installed Ollama models (for the summary-model dropdown in Settings).
#[tauri::command]
pub async fn meeting_log_list_models() -> Result<Vec<String>, String> {
    super::models::list_installed_models().await
}

/// Override the summary model used for meeting-log summaries (None = use env).
#[tauri::command]
pub fn meeting_log_set_summary_model(model: Option<String>) {
    super::models::set_summary_override(model);
}

/// Resolve the summary model that will actually be used right now.
#[tauri::command]
pub async fn meeting_log_get_summary_model() -> String {
    super::models::resolve_summary_model().await
}

/// Reveal a transcript/summary file in Finder (macOS) / Explorer.
#[tauri::command]
pub fn meeting_log_reveal(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            std::process::Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
