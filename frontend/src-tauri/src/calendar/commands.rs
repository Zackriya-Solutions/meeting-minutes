use super::models::{
    CalendarAccountStatusDto, CalendarAutoStartSettingsDto, CalendarEventDto,
    UpdateAutoStartSettingsRequest,
};
use super::oauth;
use crate::database::repositories::calendar::CalendarRepository;
use crate::state::AppState;
use chrono::{Duration as ChronoDuration, Utc};

fn status_dto_from_account(account: Option<crate::database::models::CalendarAccount>) -> CalendarAccountStatusDto {
    match account {
        Some(acc) => CalendarAccountStatusDto {
            connected: acc.status == "connected",
            email: Some(acc.email),
            status: acc.status,
        },
        None => CalendarAccountStatusDto {
            connected: false,
            email: None,
            status: "disconnected".to_string(),
        },
    }
}

/// Runs the full OAuth loopback flow and persists the resulting tokens. Blocks (async)
/// until the user finishes (or abandons) the browser sign-in.
#[tauri::command]
pub async fn calendar_connect(
    state: tauri::State<'_, AppState>,
) -> Result<CalendarAccountStatusDto, String> {
    let pkce = oauth::generate_pkce();
    let state_token = oauth::generate_state();
    let (listener, port) = oauth::bind_loopback_listener().await?;
    let auth_url = oauth::build_auth_url(port, &state_token, &pkce.challenge)?;

    log::info!("Opening browser for Google Calendar sign-in");
    crate::api::open_external_url(auth_url).await?;

    let code = oauth::await_callback(listener, &state_token).await?;
    let tokens = oauth::exchange_code(&code, &pkce.verifier, port).await?;
    let email = oauth::fetch_connected_email(&tokens.access_token).await?;

    let pool = state.db_manager.pool();
    let expires_at = Utc::now() + ChronoDuration::seconds(tokens.expires_in);
    CalendarRepository::upsert_account(
        pool,
        &email,
        &tokens.access_token,
        &tokens.refresh_token,
        expires_at,
        oauth::SCOPE,
    )
    .await
    .map_err(|e| e.to_string())?;

    log::info!("Google Calendar connected: {}", email);

    Ok(CalendarAccountStatusDto {
        connected: true,
        email: Some(email),
        status: "connected".to_string(),
    })
}

#[tauri::command]
pub async fn calendar_disconnect(state: tauri::State<'_, AppState>) -> Result<(), String> {
    CalendarRepository::disconnect(state.db_manager.pool())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn calendar_get_status(
    state: tauri::State<'_, AppState>,
) -> Result<CalendarAccountStatusDto, String> {
    let account = CalendarRepository::get_account(state.db_manager.pool())
        .await
        .map_err(|e| e.to_string())?;
    Ok(status_dto_from_account(account))
}

/// Meetings synced from the connected calendar in the next 24h (plus a 1h lookback so
/// an in-progress meeting doesn't disappear from the list).
#[tauri::command]
pub async fn calendar_get_upcoming_events(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<CalendarEventDto>, String> {
    let pool = state.db_manager.pool();
    let now = Utc::now();
    let events = CalendarRepository::get_events_in_range(
        pool,
        now - ChronoDuration::hours(1),
        now + ChronoDuration::hours(24),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(events
        .into_iter()
        .map(|e| CalendarEventDto {
            id: e.id,
            title: e.title.unwrap_or_else(|| "Untitled meeting".to_string()),
            start_time: e.start_time.to_rfc3339(),
            end_time: e.end_time.to_rfc3339(),
            is_meeting: e.meeting_url.is_some(),
            meeting_url: e.meeting_url,
            meeting_provider: e.meeting_provider,
        })
        .collect())
}

#[tauri::command]
pub async fn calendar_get_auto_start_settings(
    state: tauri::State<'_, AppState>,
) -> Result<CalendarAutoStartSettingsDto, String> {
    let account = CalendarRepository::get_account(state.db_manager.pool())
        .await
        .map_err(|e| e.to_string())?;

    Ok(match account {
        Some(acc) => CalendarAutoStartSettingsDto {
            enabled: acc.auto_start_enabled,
            mode: acc.auto_start_mode,
            grace_minutes: acc.auto_stop_grace_minutes,
        },
        // No account connected yet — report the same defaults the migration seeds.
        None => CalendarAutoStartSettingsDto {
            enabled: false,
            mode: "ask".to_string(),
            grace_minutes: 5,
        },
    })
}

#[tauri::command]
pub async fn calendar_update_auto_start_settings(
    state: tauri::State<'_, AppState>,
    settings: UpdateAutoStartSettingsRequest,
) -> Result<(), String> {
    if settings.mode != "ask" && settings.mode != "silent" {
        return Err(format!("Invalid auto-start mode '{}'", settings.mode));
    }

    CalendarRepository::update_auto_start_settings(
        state.db_manager.pool(),
        settings.enabled,
        &settings.mode,
        settings.grace_minutes,
    )
    .await
    .map_err(|e| e.to_string())
}

/// Called by the frontend right after it actually starts a calendar-triggered recording,
/// so the poller knows which event the active recording belongs to (for auto-stop) and
/// doesn't mistake a later, unrelated manual recording for this event's auto-stop target.
#[tauri::command]
pub async fn calendar_confirm_auto_start(event_id: String) -> Result<(), String> {
    super::poller::set_active_auto_event(Some(event_id));
    Ok(())
}
