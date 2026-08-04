//! Tauri commands for the Telegram share flow. See [`crate::telegram`] for why this is a
//! deep link rather than a Bot API call.

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager, Runtime, State};

use crate::database::repositories::meeting::MeetingsRepository;
use crate::llm::PrivacyConfig;
use crate::state::AppState;
use crate::telegram::share::{share_urls, DRAFT_TEXT_BUDGET};

/// Refuse to hand meeting content to another application while the user has asked for
/// local-only operation. Fails closed: an unreadable privacy setting blocks the share.
async fn ensure_sharing_allowed(pool: &sqlx::SqlitePool) -> Result<(), String> {
    // Checked directly rather than through `llm::ensure_outbound_allowed`: the per-purpose
    // toggles there govern LLM calls, and none of them describes this.
    let privacy = PrivacyConfig::load(pool)
        .await
        .map_err(|e| format!("Privacy settings unavailable: {e}"))?;
    if privacy.local_only {
        return Err("Локальный режим включён — отправка в Telegram отключена".to_string());
    }
    Ok(())
}

/// Open the Telegram chat picker with `text` prefilled. Returns once the client has been
/// launched — the user still chooses a chat, reviews the draft, and presses send inside
/// Telegram, so a successful return means "Telegram opened", never "the summary was
/// delivered". The draft's first line is [`crate::telegram::share::SHARE_URL_LINE`], which
/// the share action requires; see that module for why it cannot be omitted.
///
/// `text` is a short draft only — a meeting's title and date. It is capped at
/// [`DRAFT_TEXT_BUDGET`] because longer links get mangled silently; the summary body
/// reaches the chat through the clipboard.
#[tauri::command]
pub async fn telegram_share_text(state: State<'_, AppState>, text: String) -> Result<(), String> {
    ensure_sharing_allowed(state.db_manager.pool()).await?;

    let text = text.trim();
    if text.is_empty() {
        return Err("Нечего отправлять: суммаризация пуста".to_string());
    }
    // Hard stop rather than a best-effort send: past this size Telegram truncates the
    // draft, or stops percent-decoding it and shows raw `%D0%9A…`, in both cases without
    // reporting anything. Callers put the summary body on the clipboard instead.
    let length = text.chars().count();
    if length > DRAFT_TEXT_BUDGET {
        return Err(format!(
            "Черновик для Telegram слишком длинный ({length} > {DRAFT_TEXT_BUDGET} символов)"
        ));
    }

    let [scheme_url, web_url] = share_urls(text);
    match open_url(&scheme_url) {
        Ok(()) => {
            log::info!("[telegram] opened chat picker via tg:// ({length} chars)");
            Ok(())
        }
        Err(scheme_err) => {
            // Telegram is probably not installed; t.me works in a browser.
            log::info!("[telegram] tg:// unavailable ({scheme_err}); falling back to t.me");
            open_url(&web_url)
                .map_err(|web_err| format!("Не удалось открыть Telegram: {scheme_err}; {web_err}"))
        }
    }
}

/// Write `markdown` to a `.md` file for summaries too long to travel inside a deep link,
/// and return its path. The caller reveals it so the user can drag it into the chat.
///
/// Location mirrors the analytics report: the meeting's own folder when it has one, else
/// `{app_data_dir}/summaries/{meeting_id}/`.
#[tauri::command]
pub async fn save_summary_markdown_file<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    meeting_id: String,
    markdown: String,
) -> Result<String, String> {
    if markdown.trim().is_empty() {
        return Err("Нечего сохранять: суммаризация пуста".to_string());
    }

    let folder_path =
        MeetingsRepository::get_meeting_metadata(state.db_manager.pool(), &meeting_id)
            .await
            .map_err(|e| format!("Failed to load meeting: {e}"))?
            .and_then(|m| m.folder_path);

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let filename = format!("summary_{ts}.md");

    // 1) Meeting's own folder, alongside its audio and transcript.
    if let Some(fp) = folder_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let dir = PathBuf::from(fp);
        if dir.is_dir() || std::fs::create_dir_all(&dir).is_ok() {
            let path = dir.join(&filename);
            match std::fs::write(&path, &markdown) {
                Ok(()) => return Ok(path.to_string_lossy().to_string()),
                Err(e) => log::warn!(
                    "[telegram] could not write into meeting folder {fp}: {e}; using app-data fallback"
                ),
            }
        } else {
            log::warn!("[telegram] meeting folder {fp} is unusable; using app-data fallback");
        }
    }

    // 2) Fallback under the app data directory.
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir unavailable: {e}"))?;
    let dir = base.join("summaries").join(sanitize_component(&meeting_id));
    std::fs::create_dir_all(&dir).map_err(|e| format!("create summary dir failed: {e}"))?;
    let path = dir.join(&filename);
    std::fs::write(&path, &markdown).map_err(|e| format!("write summary file failed: {e}"))?;
    Ok(path.to_string_lossy().to_string())
}

fn sanitize_component(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Hand `url` to the OS URL handler, waiting for the launcher's exit status so an
/// unclaimed `tg://` scheme is reported as an error rather than swallowed.
fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = Command::new("open");
        c.arg(url);
        c
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        // The empty string is `start`'s window-title argument; without it a quoted URL is
        // taken as the title and nothing opens.
        let mut c = Command::new("cmd");
        c.args(["/C", "start", ""]).arg(url);
        c
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        c
    };

    let status = command
        .status()
        .map_err(|e| format!("не удалось запустить обработчик ссылок: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("обработчик ссылок вернул {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_component_strips_path_separators() {
        assert_eq!(sanitize_component("meeting-abc_123"), "meeting-abc_123");
        assert_eq!(sanitize_component("../../etc/passwd"), "______etc_passwd");
    }
}
