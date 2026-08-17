use serde::{Deserialize, Serialize};

// ---- Google API wire types ----

#[derive(Debug, Deserialize)]
pub struct GoogleTokenResponse {
    pub access_token: String,
    /// Only present on the *first* code exchange (with prompt=consent); absent on refresh.
    pub refresh_token: Option<String>,
    pub expires_in: i64,
    #[allow(dead_code)]
    pub scope: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleUserInfo {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct GoogleEventsListResponse {
    #[serde(default)]
    pub items: Vec<GoogleEvent>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleEvent {
    pub id: String,
    pub summary: Option<String>,
    pub start: Option<GoogleEventDateTime>,
    pub end: Option<GoogleEventDateTime>,
    pub location: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "hangoutLink")]
    pub hangout_link: Option<String>,
    #[serde(rename = "conferenceData")]
    pub conference_data: Option<GoogleConferenceData>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleEventDateTime {
    /// RFC3339 timestamp for timed events. Absent (with `date` set instead) for all-day events.
    #[serde(rename = "dateTime")]
    pub date_time: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleConferenceData {
    #[serde(rename = "entryPoints", default)]
    pub entry_points: Vec<GoogleEntryPoint>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleEntryPoint {
    pub uri: Option<String>,
    #[serde(rename = "entryPointType")]
    pub entry_point_type: Option<String>,
}

// ---- Frontend-facing DTOs ----

#[derive(Debug, Clone, Serialize)]
pub struct CalendarAccountStatusDto {
    pub connected: bool,
    pub email: Option<String>,
    /// "connected" | "needs_reauth" | "disconnected"
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarAutoStartSettingsDto {
    pub enabled: bool,
    /// "ask" | "silent"
    pub mode: String,
    pub grace_minutes: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAutoStartSettingsRequest {
    pub enabled: bool,
    pub mode: String,
    pub grace_minutes: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalendarEventDto {
    pub id: String,
    pub title: String,
    pub start_time: String,
    pub end_time: String,
    pub meeting_url: Option<String>,
    pub meeting_provider: Option<String>,
    pub is_meeting: bool,
}
