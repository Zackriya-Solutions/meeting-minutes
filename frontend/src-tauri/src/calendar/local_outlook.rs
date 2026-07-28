//! Public Tauri surface for the local classic-Outlook calendar.
//!
//! This integration intentionally has no HTTP client, OAuth flow, or cloud
//! fallback. On Windows it reads the calendar through the Outlook Object Model
//! exposed by the locally installed classic Outlook application. On macOS it
//! reads only visible Outlook calendar Accessibility labels after explicit user
//! approval. Events live only in the command response and are not persisted by
//! this module.

use serde::Serialize;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const UNSUPPORTED_MESSAGE: &str = "Local Outlook Calendar requires classic Outlook for Windows. \
     macOS builds use the separate Accessibility connector.";

#[derive(Debug, Clone, Serialize)]
pub struct LocalOutlookCalendarStatus {
    pub supported: bool,
    pub installed: bool,
    pub running: bool,
    pub accessibility_granted: bool,
    pub provider: &'static str,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalOutlookMeeting {
    pub id: String,
    pub calendar_id: String,
    pub calendar_name: String,
    pub store_name: String,
    pub subject: String,
    pub start_at: String,
    pub end_at: String,
    pub is_all_day: bool,
    pub is_meeting: bool,
    pub is_recurring: bool,
    pub location: Option<String>,
    pub response_status: String,
}

#[tauri::command]
pub async fn local_outlook_calendar_status() -> Result<LocalOutlookCalendarStatus, String> {
    #[cfg(target_os = "windows")]
    {
        return tauri::async_runtime::spawn_blocking(super::windows_outlook::status)
            .await
            .map_err(|error| format!("local Outlook status task failed: {error}"))?;
    }

    #[cfg(target_os = "macos")]
    {
        return tauri::async_runtime::spawn_blocking(super::macos_outlook::status)
            .await
            .map_err(|error| format!("local Outlook status task failed: {error}"));
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    Ok(LocalOutlookCalendarStatus {
        supported: false,
        installed: false,
        running: false,
        accessibility_granted: false,
        provider: "local-classic-outlook",
        detail: UNSUPPORTED_MESSAGE.to_string(),
    })
}

#[tauri::command]
pub async fn request_outlook_accessibility_permission() -> Result<LocalOutlookCalendarStatus, String>
{
    #[cfg(target_os = "macos")]
    {
        return tauri::async_runtime::spawn_blocking(
            super::macos_outlook::request_accessibility_permission,
        )
        .await
        .map_err(|error| format!("Accessibility permission task failed: {error}"))?;
    }

    #[cfg(not(target_os = "macos"))]
    local_outlook_calendar_status().await
}

#[tauri::command]
pub async fn get_upcoming_local_outlook_meetings(
    days: Option<u32>,
) -> Result<Vec<LocalOutlookMeeting>, String> {
    let days = days.unwrap_or(7).clamp(1, 31);

    #[cfg(target_os = "windows")]
    {
        return tauri::async_runtime::spawn_blocking(move || {
            super::windows_outlook::upcoming_meetings(days)
        })
        .await
        .map_err(|error| format!("local Outlook calendar task failed: {error}"))?;
    }

    #[cfg(target_os = "macos")]
    {
        return tauri::async_runtime::spawn_blocking(move || {
            super::macos_outlook::upcoming_meetings(days)
        })
        .await
        .map_err(|error| format!("local Outlook calendar task failed: {error}"))?;
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = days;
        Err(UNSUPPORTED_MESSAGE.to_string())
    }
}
