//! Public Tauri surface for the local classic-Outlook calendar.
//!
//! This integration intentionally has no HTTP client, OAuth flow, or cloud
//! fallback. On Windows it reads the calendar through the Outlook Object Model
//! exposed by the locally installed classic Outlook application. Events live
//! only in the command response and are not persisted by this module.

use serde::Serialize;

const UNSUPPORTED_MESSAGE: &str = "Local Outlook Calendar requires classic Outlook for Windows. \
     Microsoft does not expose the Outlook Object Model in new Outlook.";

#[derive(Debug, Clone, Serialize)]
pub struct LocalOutlookCalendarStatus {
    pub supported: bool,
    pub installed: bool,
    pub running: bool,
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

    #[cfg(not(target_os = "windows"))]
    Ok(LocalOutlookCalendarStatus {
        supported: false,
        installed: false,
        running: false,
        provider: "local-classic-outlook",
        detail: UNSUPPORTED_MESSAGE.to_string(),
    })
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

    #[cfg(not(target_os = "windows"))]
    {
        let _ = days;
        Err(UNSUPPORTED_MESSAGE.to_string())
    }
}
