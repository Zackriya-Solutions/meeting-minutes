// Minimal Google Calendar API v3 client — only what's needed to find Meet-linked events.

use super::{Attendee, CalendarEvent};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

const EVENTS_ENDPOINT: &str = "https://www.googleapis.com/calendar/v3/calendars/primary/events";

#[derive(Debug, Deserialize)]
struct EventsListResponse {
    #[serde(default)]
    items: Vec<RawEvent>,
}

#[derive(Debug, Deserialize)]
struct RawEvent {
    id: String,
    #[serde(default)]
    summary: Option<String>,
    start: RawEventTime,
    end: RawEventTime,
    #[serde(default)]
    attendees: Vec<RawAttendee>,
    #[serde(default, rename = "conferenceData")]
    conference_data: Option<RawConferenceData>,
}

#[derive(Debug, Deserialize)]
struct RawEventTime {
    #[serde(rename = "dateTime")]
    date_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct RawAttendee {
    email: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawConferenceData {
    #[serde(default, rename = "entryPoints")]
    entry_points: Vec<RawEntryPoint>,
}

#[derive(Debug, Deserialize)]
struct RawEntryPoint {
    #[serde(rename = "entryPointType")]
    entry_point_type: String,
    uri: String,
}

/// Fetch Google Calendar events in `[time_min, time_max]` that carry a Google Meet link.
/// Events without a video conferencing entry point (e.g. plain in-person events) are dropped —
/// they're outside this integration's scope.
pub async fn list_meet_events(
    access_token: &str,
    time_min: DateTime<Utc>,
    time_max: DateTime<Utc>,
) -> Result<Vec<CalendarEvent>> {
    let client = reqwest::Client::new();
    let response = client
        .get(EVENTS_ENDPOINT)
        .bearer_auth(access_token)
        .query(&[
            ("timeMin", time_min.to_rfc3339()),
            ("timeMax", time_max.to_rfc3339()),
            ("singleEvents", "true".to_string()),
            ("orderBy", "startTime".to_string()),
            // Without this, Google omits `conferenceData` entirely from every event regardless
            // of whether it has a Meet link — which silently broke all Meet-link matching.
            ("conferenceDataVersion", "1".to_string()),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(anyhow!("Google Calendar API error: {}", text));
    }

    let parsed: EventsListResponse = response.json().await?;

    let events = parsed
        .items
        .into_iter()
        .filter_map(|raw| {
            let start = raw.start.date_time?;
            let end = raw.end.date_time?;
            let meet_link = raw.conference_data.as_ref().and_then(|cd| {
                cd.entry_points
                    .iter()
                    .find(|ep| ep.entry_point_type == "video")
                    .map(|ep| ep.uri.clone())
            });
            meet_link.as_ref()?;

            Some(CalendarEvent {
                id: raw.id,
                title: raw.summary.unwrap_or_else(|| "Untitled event".to_string()),
                start_time: start,
                end_time: end,
                meet_link,
                attendees: raw
                    .attendees
                    .into_iter()
                    .map(|a| Attendee {
                        name: a.display_name,
                        email: a.email,
                    })
                    .collect(),
            })
        })
        .collect();

    Ok(events)
}
