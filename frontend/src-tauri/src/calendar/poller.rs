use super::{client, oauth};
use crate::database::models::CalendarEvent as CalendarEventRow;
use crate::database::repositories::calendar::CalendarRepository;
use crate::state::AppState;
use chrono::{Duration as ChronoDuration, Utc};
use std::sync::Mutex as StdMutex;
use std::time::Duration;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager, Runtime};

const POLL_INTERVAL: Duration = Duration::from_secs(60);
/// Auto-start fires once per event, the first tick where `now` falls in
/// `[start_time, start_time + AUTO_START_WINDOW)`.
const AUTO_START_WINDOW: ChronoDuration = ChronoDuration::minutes(5);

static POLL_TASK: StdMutex<Option<JoinHandle<()>>> = StdMutex::new(None);

/// The calendar event id (if any) that the currently-active recording was
/// auto-started for. Set by the frontend via `calendar_confirm_auto_start` once it has
/// actually started recording; cleared once that recording ends (for any reason — see
/// `sync_once`'s self-healing check). This is what lets auto-stop target the *specific*
/// event's recording rather than stopping whatever happens to be recording when its
/// end time + grace period arrives.
static ACTIVE_AUTO_EVENT: StdMutex<Option<String>> = StdMutex::new(None);

pub fn set_active_auto_event(event_id: Option<String>) {
    if let Ok(mut slot) = ACTIVE_AUTO_EVENT.lock() {
        *slot = event_id;
    }
}

fn active_auto_event() -> Option<String> {
    ACTIVE_AUTO_EVENT.lock().ok().and_then(|g| g.clone())
}

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
        account.access_token.clone()
    };

    let now = Utc::now();

    // Self-healing: if nothing is recording anymore (stopped manually, crashed, whatever),
    // any event we thought was "the active auto-started one" no longer is.
    if !crate::is_recording().await {
        set_active_auto_event(None);
    }

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
            continue;
        }

        if account.auto_start_enabled {
            evaluate_triggers(app, pool, &row, &account, now, start_time, end_time).await;
        }
    }

    Ok(())
}

/// Decides whether this event should auto-start or auto-stop recording on this tick.
/// Idempotent across ticks via `triggered_start_at`/`triggered_stop_at`.
async fn evaluate_triggers<R: Runtime>(
    app: &AppHandle<R>,
    pool: &sqlx::SqlitePool,
    row: &CalendarEventRow,
    account: &crate::database::models::CalendarAccount,
    now: chrono::DateTime<Utc>,
    start_time: chrono::DateTime<Utc>,
    end_time: chrono::DateTime<Utc>,
) {
    // Re-read the stored row: `upsert_event` doesn't touch triggered_start_at/
    // triggered_stop_at on conflict, so this reflects any prior tick's decision.
    let stored = match CalendarRepository::get_event(pool, &row.id).await {
        Ok(Some(stored)) => stored,
        _ => return,
    };

    let title = row
        .title
        .clone()
        .unwrap_or_else(|| "Untitled meeting".to_string());

    if stored.triggered_start_at.is_none() && now >= start_time && now < start_time + AUTO_START_WINDOW {
        // Mark decided *before* emitting so a slow/unresponded prompt can't re-fire next tick.
        let _ = CalendarRepository::mark_trigger_started(pool, &row.id).await;

        if crate::is_recording().await {
            log::info!(
                "Calendar event '{}' started but a recording is already in progress — skipping auto-start",
                title
            );
        } else {
            log::info!("Calendar event '{}' entered its auto-start window", title);
            let payload = serde_json::json!({
                "eventId": row.id,
                "title": title,
                "mode": account.auto_start_mode,
            });
            if let Err(e) = app.emit("calendar-auto-start-pending", payload) {
                log::warn!("Failed to emit calendar-auto-start-pending: {}", e);
            }
        }
        return;
    }

    let auto_stop_due = stored.triggered_start_at.is_some()
        && stored.triggered_stop_at.is_none()
        && active_auto_event().as_deref() == Some(row.id.as_str())
        && now >= end_time + ChronoDuration::minutes(account.auto_stop_grace_minutes);

    if auto_stop_due && crate::is_recording().await {
        let _ = CalendarRepository::mark_trigger_stopped(pool, &row.id).await;
        set_active_auto_event(None);

        let data_dir = match app.path().app_data_dir() {
            Ok(dir) => dir,
            Err(e) => {
                log::error!("Auto-stop: failed to get app data dir: {}", e);
                return;
            }
        };
        let timestamp = chrono::Local::now().format("%Y-%m-%dT%H-%M-%S").to_string();
        let save_path = data_dir.join(format!("recording-{}.wav", timestamp));

        log::info!("Auto-stopping recording for calendar event '{}' (ended + grace period elapsed)", title);
        match crate::audio::recording_commands::stop_recording(
            app.clone(),
            crate::audio::recording_commands::RecordingArgs {
                save_path: save_path.to_string_lossy().to_string(),
            },
        )
        .await
        {
            Ok(_) => {
                if let Err(e) = app.emit("recording-stop-complete", true) {
                    log::error!("Auto-stop: failed to emit recording-stop-complete: {}", e);
                }
            }
            Err(e) => log::error!("Auto-stop failed for calendar event '{}': {}", row.id, e),
        }
    }
}
