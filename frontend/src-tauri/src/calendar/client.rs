use super::models::{GoogleEvent, GoogleEventsListResponse};
use chrono::{DateTime, Utc};
use regex::Regex;
use std::time::Duration;

const EVENTS_ENDPOINT: &str = "https://www.googleapis.com/calendar/v3/calendars/primary/events";

/// Fetches events on the primary calendar between `time_min` and `time_max`.
pub async fn list_events(
    access_token: &str,
    time_min: DateTime<Utc>,
    time_max: DateTime<Utc>,
) -> Result<Vec<GoogleEvent>, String> {
    let client = reqwest::Client::new();
    let response = client
        .get(EVENTS_ENDPOINT)
        .bearer_auth(access_token)
        .query(&[
            ("timeMin", time_min.to_rfc3339()),
            ("timeMax", time_max.to_rfc3339()),
            ("singleEvents", "true".to_string()),
            ("orderBy", "startTime".to_string()),
            ("maxResults", "25".to_string()),
        ])
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Failed to reach Google Calendar API: {}", e))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err("unauthorized".to_string());
    }
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Google Calendar API error ({}): {}", status, body));
    }

    let parsed: GoogleEventsListResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Google Calendar API response: {}", e))?;

    Ok(parsed.items)
}

/// A meeting link found on an event, and which provider it belongs to.
pub struct MeetingLink {
    pub url: String,
    pub provider: String,
}

fn provider_for_url(url: &str) -> Option<&'static str> {
    if url.contains("meet.google.com") {
        Some("meet")
    } else if url.contains("zoom.us") {
        Some("zoom")
    } else if url.contains("teams.microsoft.com") {
        Some("teams")
    } else {
        None
    }
}

/// Determines whether an event is an actionable "meeting" — one with a Meet/Zoom/Teams
/// link, either from Calendar's structured conferencing data or found in free text.
pub fn detect_meeting_link(event: &GoogleEvent) -> Option<MeetingLink> {
    if let Some(link) = &event.hangout_link {
        if let Some(provider) = provider_for_url(link) {
            return Some(MeetingLink {
                url: link.clone(),
                provider: provider.to_string(),
            });
        }
    }

    if let Some(conference) = &event.conference_data {
        for entry in &conference.entry_points {
            if let Some(uri) = &entry.uri {
                if let Some(provider) = provider_for_url(uri) {
                    return Some(MeetingLink {
                        url: uri.clone(),
                        provider: provider.to_string(),
                    });
                }
            }
        }
    }

    let url_re = Regex::new(r#"https?://[^\s<>"]+"#).ok()?;
    for haystack in [event.location.as_deref(), event.description.as_deref()]
        .into_iter()
        .flatten()
    {
        for candidate in url_re.find_iter(haystack) {
            if let Some(provider) = provider_for_url(candidate.as_str()) {
                return Some(MeetingLink {
                    url: candidate.as_str().to_string(),
                    provider: provider.to_string(),
                });
            }
        }
    }

    None
}
