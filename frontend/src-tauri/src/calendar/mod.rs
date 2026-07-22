pub mod client;
pub mod commands;
pub mod matcher;
pub mod oauth;

use serde::{Deserialize, Serialize};

/// Google OAuth client credentials + token bundle for Calendar API access.
/// Persisted as a single JSON blob in `settings.googleCalendarConfig`, mirroring
/// how `CustomOpenAIConfig` is stored in `summary::CustomOpenAIConfig`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleCalendarConfig {
    pub client_id: String,
    pub client_secret: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    /// RFC3339 timestamp of access_token expiry
    pub token_expiry: Option<String>,
    pub scope: Option<String>,
}

impl GoogleCalendarConfig {
    pub fn is_connected(&self) -> bool {
        self.access_token.is_some() && self.refresh_token.is_some()
    }
}

/// A Google Calendar event carrying a Google Meet link.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub end_time: chrono::DateTime<chrono::Utc>,
    pub meet_link: Option<String>,
    pub attendees: Vec<Attendee>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attendee {
    pub name: Option<String>,
    pub email: String,
}
