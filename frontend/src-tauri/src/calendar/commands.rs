use crate::calendar::{client, matcher, oauth, CalendarEvent};
use crate::database::repositories::meeting::MeetingsRepository;
use crate::database::repositories::setting::SettingsRepository;
use crate::state::AppState;
use chrono::Duration;
use log::info;
use serde::Serialize;
use sqlx::SqlitePool;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Runtime};

#[derive(Debug, Serialize)]
pub struct CalendarConnectionStatus {
    pub connected: bool,
    pub scope: Option<String>,
}

/// Guards against overlapping OAuth flows — e.g. a page reload resetting the frontend's
/// "connecting" state while a previous `calendar_start_oauth` call is still waiting (up to 3
/// minutes) on its own loopback listener in the background. Without this, repeated clicks pile
/// up stray listener threads and open a new browser tab each time.
static OAUTH_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Kick off the Google OAuth loopback flow: open the system browser for consent, catch the
/// redirect locally, exchange the code for tokens, and persist them.
#[tauri::command]
pub async fn calendar_start_oauth<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    client_id: String,
    client_secret: String,
) -> Result<CalendarConnectionStatus, String> {
    if OAUTH_IN_PROGRESS
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(
            "A Google sign-in is already in progress. Finish it in the browser tab that opened, \
             or wait up to 3 minutes for it to time out before trying again."
                .to_string(),
        );
    }

    let pool = state.db_manager.pool().clone();
    let result = run_oauth_flow(pool, client_id, client_secret).await;
    OAUTH_IN_PROGRESS.store(false, Ordering::SeqCst);
    result
}

async fn run_oauth_flow(
    pool: SqlitePool,
    client_id: String,
    client_secret: String,
) -> Result<CalendarConnectionStatus, String> {
    let (listener, port) = oauth::bind_loopback_listener()
        .map_err(|e| format!("Failed to start local OAuth listener: {}", e))?;
    let redirect_uri = format!("http://127.0.0.1:{}/callback", port);
    let authorize_url = oauth::build_authorize_url(&client_id, &redirect_uri);

    info!("Opening browser for Google Calendar OAuth consent");
    crate::api::open_external_url(authorize_url)
        .await
        .map_err(|e| format!("Failed to open browser for Google sign-in: {}", e))?;

    let code = tokio::task::spawn_blocking(move || {
        oauth::wait_for_authorization_code(listener, std::time::Duration::from_secs(180))
    })
    .await
    .map_err(|e| format!("OAuth listener task failed: {}", e))?
    .map_err(|e| format!("Google sign-in was not completed: {}", e))?;

    let config = oauth::exchange_code_for_tokens(&client_id, &client_secret, &code, &redirect_uri)
        .await
        .map_err(|e| format!("Failed to exchange authorization code: {}", e))?;

    SettingsRepository::save_google_calendar_config(&pool, &config)
        .await
        .map_err(|e| format!("Failed to save Google Calendar credentials: {}", e))?;

    info!("Google Calendar connected successfully");
    Ok(CalendarConnectionStatus {
        connected: true,
        scope: config.scope,
    })
}

#[tauri::command]
pub async fn calendar_disconnect<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    SettingsRepository::clear_google_calendar_config(state.db_manager.pool())
        .await
        .map_err(|e| format!("Failed to disconnect Google Calendar: {}", e))
}

#[tauri::command]
pub async fn calendar_get_connection_status<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
) -> Result<CalendarConnectionStatus, String> {
    let config = SettingsRepository::get_google_calendar_config(state.db_manager.pool())
        .await
        .map_err(|e| format!("Failed to read Google Calendar config: {}", e))?;

    Ok(match config {
        Some(config) if config.is_connected() => CalendarConnectionStatus {
            connected: true,
            scope: config.scope,
        },
        _ => CalendarConnectionStatus {
            connected: false,
            scope: None,
        },
    })
}

/// Match a just-recorded meeting against Google Calendar and write attendee/title/meet-link
/// metadata onto its row. Called by the frontend right after a transcript (and thus the
/// meeting row) has been saved — meeting rows don't exist until then (see recording_commands.rs).
/// Returns `Ok(None)` (not an error) when Calendar isn't connected or no event matches, since
/// that's an expected, non-fatal outcome for meetings that weren't scheduled via Calendar.
#[tauri::command]
pub async fn api_save_meeting_calendar_metadata<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Option<CalendarEvent>, String> {
    let pool = state.db_manager.pool();

    let meeting = MeetingsRepository::get_meeting_metadata(pool, &meeting_id)
        .await
        .map_err(|e| format!("Failed to load meeting: {}", e))?
        .ok_or_else(|| format!("No meeting found with id {}", meeting_id))?;

    let config = SettingsRepository::get_google_calendar_config(pool)
        .await
        .map_err(|e| format!("Failed to read Google Calendar config: {}", e))?;

    let Some(config) = config.filter(|c| c.is_connected()) else {
        return Ok(None);
    };

    let access_token = oauth::ensure_valid_access_token(pool, &config)
        .await
        .map_err(|e| format!("Failed to refresh Google Calendar access token: {}", e))?;

    // `meeting.created_at` is set when the transcript is saved — i.e. roughly when the recording
    // *stopped*, not when the meeting started (see transcript.rs's INSERT). For a long meeting
    // that drift can be significant, so this window has to be wide enough to still contain the
    // event's actual start; a narrow (e.g. ±15min) secondary filter would cause false "no match"
    // results for anything longer than that. `find_matching_event`'s greatest-overlap scoring
    // already picks the best candidate among whatever's fetched, so one generous window for both
    // fetching and matching is both simpler and more correct than fetching wide then filtering
    // tight.
    let meeting_time = meeting.created_at.0;
    let window_start = meeting_time - Duration::hours(2);
    let window_end = meeting_time + Duration::hours(2);
    let events = client::list_meet_events(&access_token, window_start, window_end)
        .await
        .map_err(|e| format!("Failed to fetch Google Calendar events: {}", e))?;

    let Some(matched) = matcher::find_matching_event(&events, window_start, window_end) else {
        return Ok(None);
    };

    let attendees_json = serde_json::to_string(&matched.attendees)
        .map_err(|e| format!("Failed to serialize attendees: {}", e))?;

    MeetingsRepository::update_meeting_calendar_metadata(
        pool,
        &meeting_id,
        &matched.id,
        &attendees_json,
        matched.meet_link.as_deref(),
        &matched.start_time.to_rfc3339(),
        &matched.end_time.to_rfc3339(),
    )
    .await
    .map_err(|e| format!("Failed to save calendar metadata: {}", e))?;

    // Only override the generic auto-generated title, never a name the user already set.
    if meeting.title.starts_with("Meeting ") {
        let _ = MeetingsRepository::update_meeting_title(pool, &meeting_id, &matched.title).await;
    }

    Ok(Some(matched.clone()))
}
