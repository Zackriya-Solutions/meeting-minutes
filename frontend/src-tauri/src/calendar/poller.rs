use super::{client, oauth};
use crate::database::models::CalendarEvent as CalendarEventRow;
use crate::database::repositories::calendar::CalendarRepository;
use crate::state::AppState;
use chrono::{Duration as ChronoDuration, Utc};
use std::sync::Mutex as StdMutex;
use std::time::Duration;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager, Runtime};

const POLL_INTERVAL: Duration = Duration::from_secs(60);

static POLL_TASK: StdMutex<Option<JoinHandle<()>>> = StdMutex::new(None);

/// Starts the background calendar sync loop (idempotent — replaces any previous task).
/// Safe to call even when no account is connected yet; each tick is a no-op until one is.
pub fn start<R: Runtime>(app: AppHandle<R>) {
    let handle = tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            if let Err(e) = sync_once(&app).await {
                log::debug!("Calendar sync tick skipped: {}", e);
            }
        }
    });

    if let Ok(mut slot) = POLL_TASK.lock() {
        if let Some(previous) = slot.replace(handle) {
            previous.abort();
        }
    }
}

async fn sync_once<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "Database not ready yet".to_string())?;
    let pool = state.db_manager.pool();

    let account = CalendarRepository::get_account(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No calendar account connected".to_string())?;

    if account.status != "connected" {
        return Err(format!("Calendar account status is '{}'", account.status));
    }

    // Refresh proactively if the token is expired or about to be (2 min buffer).
    let access_token = if account.token_expires_at <= Utc::now() + ChronoDuration::minutes(2) {
        match oauth::refresh_access_token(&account.refresh_token).await {
            Ok(refreshed) => {
                let new_expiry = Utc::now() + ChronoDuration::seconds(refreshed.expires_in);
                let _ = CalendarRepository::update_access_token(
                    pool,
                    &refreshed.access_token,
                    new_expiry,
                )
                .await;
                refreshed.access_token
            }
            Err(e) => {
                log::warn!("Google Calendar token refresh failed, marking needs_reauth: {}", e);
                let _ = CalendarRepository::mark_needs_reauth(pool).await;
                return Err(e);
            }
        }
    } else {
        account.access_token
    };

    let now = Utc::now();
    let time_min = now - ChronoDuration::minutes(5);
    let time_max = now + ChronoDuration::hours(2);

    let events = match client::list_events(&access_token, time_min, time_max).await {
        Ok(events) => events,
        Err(e) if e == "unauthorized" => {
            log::warn!("Google Calendar API rejected the access token; marking needs_reauth");
            let _ = CalendarRepository::mark_needs_reauth(pool).await;
            return Err("Google access token was rejected".to_string());
        }
        Err(e) => return Err(e),
    };

    for event in events {
        let (start, end) = match (
            event.start.as_ref().and_then(|s| s.date_time.as_ref()),
            event.end.as_ref().and_then(|e| e.date_time.as_ref()),
        ) {
            (Some(start), Some(end)) => (start, end),
            // All-day events have no specific start/end time — nothing to trigger on, skip.
            _ => continue,
        };

        let start_time = match chrono::DateTime::parse_from_rfc3339(start) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(_) => continue,
        };
        let end_time = match chrono::DateTime::parse_from_rfc3339(end) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(_) => continue,
        };

        let meeting_link = client::detect_meeting_link(&event);
        let raw_json = serde_json::to_string(&serde_json::json!({
            "summary": event.summary.clone(),
            "location": event.location.clone(),
        }))
        .ok();

        let row = CalendarEventRow {
            id: event.id,
            calendar_account_id: account.id,
            title: event.summary,
            start_time,
            end_time,
            meeting_url: meeting_link.as_ref().map(|m| m.url.clone()),
            meeting_provider: meeting_link.as_ref().map(|m| m.provider.clone()),
            raw_json,
            triggered_start_at: None,
            triggered_stop_at: None,
            linked_meeting_id: None,
            synced_at: now,
        };

        if let Err(e) = CalendarRepository::upsert_event(pool, &row).await {
            log::warn!("Failed to persist synced calendar event: {}", e);
        }
    }

    Ok(())
}
