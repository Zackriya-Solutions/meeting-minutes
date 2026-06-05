use anyhow::{anyhow, Result};
use chrono::{DateTime, Local, Utc};
use log::info;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "obsidian_settings.json";
const STORE_KEY: &str = "settings";
const MEETINGS_FOLDER: &str = "Meetings";
const DEFAULT_FILENAME_TEMPLATE: &str = "{date} {title}.md";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ObsidianSettings {
    pub vault_path: Option<String>,
    #[serde(default = "default_filename_template")]
    pub filename_template: String,
}

impl Default for ObsidianSettings {
    fn default() -> Self {
        Self {
            vault_path: None,
            filename_template: default_filename_template(),
        }
    }
}

fn default_filename_template() -> String {
    DEFAULT_FILENAME_TEMPLATE.to_string()
}

#[derive(Debug, Serialize)]
pub struct ObsidianExportResult {
    pub file_path: String,
    pub relative_path: String,
}

pub async fn load_obsidian_settings<R: Runtime>(app: &AppHandle<R>) -> Result<ObsidianSettings> {
    let store = app
        .store(STORE_FILE)
        .map_err(|e| anyhow!("Failed to access Obsidian settings store: {}", e))?;

    if let Some(value) = store.get(STORE_KEY) {
        return serde_json::from_value::<ObsidianSettings>(value.clone())
            .map_err(|e| anyhow!("Failed to read Obsidian settings: {}", e));
    }

    Ok(ObsidianSettings::default())
}

async fn save_obsidian_settings<R: Runtime>(
    app: &AppHandle<R>,
    settings: &ObsidianSettings,
) -> Result<()> {
    let store = app
        .store(STORE_FILE)
        .map_err(|e| anyhow!("Failed to access Obsidian settings store: {}", e))?;
    let value = serde_json::to_value(settings)
        .map_err(|e| anyhow!("Failed to serialize Obsidian settings: {}", e))?;

    store.set(STORE_KEY, value);
    store
        .save()
        .map_err(|e| anyhow!("Failed to save Obsidian settings: {}", e))?;

    Ok(())
}

fn validate_vault_path(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(anyhow!("Vault path does not exist"));
    }

    if !path.is_dir() {
        return Err(anyhow!("Vault path must be a folder"));
    }

    Ok(())
}

fn meetings_path(vault_path: &str) -> PathBuf {
    PathBuf::from(vault_path).join(MEETINGS_FOLDER)
}

fn open_folder(path: &Path) -> Result<()> {
    let folder_path = path.to_string_lossy().to_string();

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&folder_path)
            .spawn()
            .map_err(|e| anyhow!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&folder_path)
            .spawn()
            .map_err(|e| anyhow!("Failed to open folder: {}", e))?;
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&folder_path)
            .spawn()
            .map_err(|e| anyhow!("Failed to open folder: {}", e))?;
    }

    Ok(())
}

fn sanitize_filename(input: &str) -> String {
    let mut filename = input
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect::<String>();

    filename = filename
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(['.', ' ', '-'])
        .to_string();

    if filename.len() > 120 {
        filename.truncate(120);
        filename = filename.trim_matches(['.', ' ', '-']).to_string();
    }

    if filename.is_empty() {
        "Untitled Meeting".to_string()
    } else {
        filename
    }
}

fn normalize_filename_template(template: Option<String>) -> String {
    template
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(default_filename_template)
}

fn render_filename_template(
    template: &str,
    meeting_id: &str,
    title: &str,
    created_at: &str,
) -> String {
    let date = parse_created_date(created_at)
        .format("%Y-%m-%d")
        .to_string();
    let short_id = meeting_id.chars().take(8).collect::<String>();
    let rendered = template
        .replace("{date}", &date)
        .replace("{title}", title)
        .replace("{id}", meeting_id)
        .replace("{short_id}", &short_id);
    let mut filename = sanitize_filename(&rendered);

    if !filename.to_lowercase().ends_with(".md") {
        filename.push_str(".md");
    }

    filename
}

fn parse_created_date(created_at: &str) -> DateTime<Local> {
    DateTime::parse_from_rfc3339(created_at)
        .map(|date| date.with_timezone(&Local))
        .unwrap_or_else(|_| Local::now())
}

fn build_obsidian_markdown(
    meeting_id: &str,
    title: &str,
    created_at: &str,
    summary_markdown: &str,
    transcript_markdown: &str,
) -> String {
    let exported_at = Utc::now().to_rfc3339();
    let title = title.trim();
    let created = parse_created_date(created_at).to_rfc3339();

    format!(
        "---\nsource: meetily\nmeeting_id: \"{}\"\ncreated: \"{}\"\nexported: \"{}\"\ntags:\n  - meetings\n---\n\n# {}\n\n## Summary\n\n{}\n\n## Transcript\n\n{}\n",
        meeting_id,
        created,
        exported_at,
        title,
        summary_markdown.trim(),
        transcript_markdown.trim()
    )
}

#[tauri::command]
pub async fn get_obsidian_settings<R: Runtime>(
    app: AppHandle<R>,
) -> Result<ObsidianSettings, String> {
    load_obsidian_settings(&app)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_obsidian_vault_path<R: Runtime>(
    app: AppHandle<R>,
    vault_path: Option<String>,
) -> Result<ObsidianSettings, String> {
    let normalized_path = vault_path
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty());

    if let Some(path) = &normalized_path {
        validate_vault_path(Path::new(path)).map_err(|e| e.to_string())?;
    }

    let mut settings = load_obsidian_settings(&app)
        .await
        .map_err(|e| e.to_string())?;
    settings.vault_path = normalized_path;
    save_obsidian_settings(&app, &settings)
        .await
        .map_err(|e| e.to_string())?;

    Ok(settings)
}

#[tauri::command]
pub async fn set_obsidian_settings<R: Runtime>(
    app: AppHandle<R>,
    vault_path: Option<String>,
    filename_template: Option<String>,
) -> Result<ObsidianSettings, String> {
    let normalized_path = vault_path
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty());

    if let Some(path) = &normalized_path {
        validate_vault_path(Path::new(path)).map_err(|e| e.to_string())?;
    }

    let settings = ObsidianSettings {
        vault_path: normalized_path,
        filename_template: normalize_filename_template(filename_template),
    };
    save_obsidian_settings(&app, &settings)
        .await
        .map_err(|e| e.to_string())?;

    Ok(settings)
}

#[tauri::command]
pub async fn open_obsidian_meetings_folder<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let settings = load_obsidian_settings(&app)
        .await
        .map_err(|e| e.to_string())?;
    let vault_path = settings
        .vault_path
        .ok_or_else(|| "Set an Obsidian vault in Settings first".to_string())?;

    validate_vault_path(Path::new(&vault_path)).map_err(|e| e.to_string())?;
    let path = meetings_path(&vault_path);
    std::fs::create_dir_all(&path)
        .map_err(|e| format!("Failed to create Meetings folder: {}", e))?;
    open_folder(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn export_meeting_to_obsidian<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    title: String,
    created_at: String,
    summary_markdown: String,
    transcript_markdown: String,
) -> Result<ObsidianExportResult, String> {
    let settings = load_obsidian_settings(&app)
        .await
        .map_err(|e| e.to_string())?;
    let vault_path = settings
        .vault_path
        .ok_or_else(|| "Set an Obsidian vault in Settings first".to_string())?;

    validate_vault_path(Path::new(&vault_path)).map_err(|e| e.to_string())?;

    let meetings_dir = meetings_path(&vault_path);
    std::fs::create_dir_all(&meetings_dir)
        .map_err(|e| format!("Failed to create Meetings folder: {}", e))?;

    let filename = render_filename_template(
        &settings.filename_template,
        &meeting_id,
        &title,
        &created_at,
    );
    let file_path = meetings_dir.join(filename);
    let markdown = build_obsidian_markdown(
        &meeting_id,
        &title,
        &created_at,
        &summary_markdown,
        &transcript_markdown,
    );

    std::fs::write(&file_path, markdown)
        .map_err(|e| format!("Failed to write Obsidian note: {}", e))?;

    info!(
        "Exported meeting {} to Obsidian: {:?}",
        meeting_id, file_path
    );

    Ok(ObsidianExportResult {
        file_path: file_path.to_string_lossy().to_string(),
        relative_path: format!(
            "{}/{}",
            MEETINGS_FOLDER,
            file_path.file_name().unwrap_or_default().to_string_lossy()
        ),
    })
}
