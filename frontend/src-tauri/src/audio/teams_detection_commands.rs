use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_notification::NotificationExt;

use crate::database::repositories::setting::SettingsRepository;
use crate::state::AppState;

#[cfg(target_os = "macos")]
use super::teams_detector::detect_teams_audio_active;
#[cfg(not(target_os = "macos"))]
use super::teams_detector::detect_teams_process_running;

/// Global flag tracking whether Teams audio was active in the last poll (macOS).
static TEAMS_AUDIO_WAS_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Global flag tracking whether Teams was running in the last process poll (Windows/Linux).
#[cfg(not(target_os = "macos"))]
static TEAMS_PROCESS_WAS_RUNNING: AtomicBool = AtomicBool::new(false);

/// Number of consecutive "not running" polls before firing teams-meeting-ended (Windows/Linux).
#[cfg(not(target_os = "macos"))]
const STOP_DEBOUNCE_POLLS: u32 = 3;

#[tauri::command]
pub async fn get_teams_detection_enabled(
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let pool = state.db_manager.pool();
    SettingsRepository::get_teams_detection_enabled(pool)
        .await
        .map_err(|e| format!("DB error: {e}"))
}

#[tauri::command]
pub async fn set_teams_detection_enabled(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let pool = state.db_manager.pool();
    SettingsRepository::set_teams_detection_enabled(pool, enabled)
        .await
        .map_err(|e| format!("DB error: {e}"))
}

/// Spawns the background Teams detection task.
///
/// macOS: registers listeners for the existing `system-audio-started` / `system-audio-stopped`
/// Tauri events and filters for Teams app names.
///
/// Windows/Linux: polls the process list every 5 seconds.
pub fn spawn_teams_detection_task<R: Runtime + 'static>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        run_teams_detection_loop(app).await;
    });
}

async fn is_detection_enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    if let Some(app_state) = app.try_state::<AppState>() {
        let pool = app_state.db_manager.pool();
        SettingsRepository::get_teams_detection_enabled(pool)
            .await
            .unwrap_or(false)
    } else {
        false
    }
}

// ─── macOS path ─────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
async fn run_teams_detection_loop<R: Runtime>(app: AppHandle<R>) {
    use tauri::Listener;

    let app_for_start = app.clone();
    let app_for_stop = app.clone();

    // Listen on the existing system-audio events (fired by start_system_audio_monitoring).
    // These events are emitted when the macOS audio subsystem detects app audio usage.
    // NOTE: `system-audio-started` fires every time the app list changes (any app starts audio),
    // so we also detect Teams-end here: when Teams drops out of the current app list while other
    // apps keep audio active (music, browser tabs, etc.). This handles the common case where
    // `system-audio-stopped` is never emitted because background audio keeps playing.
    log::info!("teams_detection: registering system-audio event listeners (macOS)");

    let _start_id = app.listen("system-audio-started", move |event| {
        let app = app_for_start.clone();
        let payload = event.payload().to_string();
        tauri::async_runtime::spawn(async move {
            log::debug!("teams_detection: system-audio-started fired, payload={}", payload);
            if !is_detection_enabled(&app).await {
                log::debug!("teams_detection: detection disabled, ignoring system-audio-started");
                return;
            }
            let apps: Vec<String> = serde_json::from_str(&payload).unwrap_or_default();
            log::info!("teams_detection: active audio apps = {:?}", apps);
            let teams_active = detect_teams_audio_active(&apps);
            log::debug!("teams_detection: teams_active={}", teams_active);

            let was_active = TEAMS_AUDIO_WAS_ACTIVE.swap(teams_active, Ordering::SeqCst);

            if teams_active && !was_active {
                let app_name = apps
                    .iter()
                    .find(|a| super::teams_detector::is_teams_app_name(a))
                    .cloned()
                    .unwrap_or_else(|| "Microsoft Teams".to_string());

                log::info!("teams_detection: Teams meeting started (app: {})", app_name);

                // Bring Meetily window to front so the popup is immediately visible.
                crate::tray::focus_main_window(&app);

                // Native OS notification so user is alerted even if Meetily is behind Teams.
                let _ = app.notification()
                    .builder()
                    .title("Meetily — Meeting detected")
                    .body("MS Teams meeting started. Click Start Recording to capture it.")
                    .show();

                // Emit event — the frontend popup asks the user whether to start recording.
                let _ = app.emit("teams-meeting-started", serde_json::json!({ "app_name": app_name }));
            } else if !teams_active && was_active {
                log::info!("teams_detection: Teams meeting ended (Teams left active audio list)");
                let _ = app.emit("teams-meeting-ended", serde_json::json!({}));
            } else {
                log::debug!("teams_detection: no state change (teams_active={}, was_active={})", teams_active, was_active);
            }
        });
    });

    let _stop_id = app.listen("system-audio-stopped", move |_event| {
        let app = app_for_stop.clone();
        tauri::async_runtime::spawn(async move {
            log::debug!("teams_detection: system-audio-stopped fired");
            if !is_detection_enabled(&app).await {
                log::debug!("teams_detection: detection disabled, ignoring system-audio-stopped");
                return;
            }
            let was_active = TEAMS_AUDIO_WAS_ACTIVE.swap(false, Ordering::SeqCst);
            if was_active {
                log::info!("teams_detection: Teams meeting ended (audio stopped)");
                let _ = app.emit("teams-meeting-ended", serde_json::json!({}));
            }
        });
    });

    // Keep the task alive — listeners are dropped when this future returns.
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

// ─── Windows / Linux path ────────────────────────────────────────────────────

#[cfg(not(target_os = "macos"))]
async fn run_teams_detection_loop<R: Runtime>(app: AppHandle<R>) {
    let mut debounce_counter: u32 = 0;

    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;

        if !is_detection_enabled(&app).await {
            // Reset state when disabled so we don't fire stale events on re-enable.
            TEAMS_PROCESS_WAS_RUNNING.store(false, Ordering::SeqCst);
            debounce_counter = 0;
            continue;
        }

        let running = tokio::task::spawn_blocking(detect_teams_process_running)
            .await
            .unwrap_or(false);

        log::debug!("teams_detection: poll — running={}", running);

        let was_running = TEAMS_PROCESS_WAS_RUNNING.load(Ordering::SeqCst);

        if running && !was_running {
            TEAMS_PROCESS_WAS_RUNNING.store(true, Ordering::SeqCst);
            debounce_counter = 0;
            log::info!("teams_detection: Teams process started");

            crate::tray::focus_main_window(&app);
            let _ = app.notification()
                .builder()
                .title("Meetily — Meeting detected")
                .body("MS Teams meeting started. Click Start Recording to capture it.")
                .show();

            let _ = app.emit("teams-meeting-started", serde_json::json!({ "app_name": "Microsoft Teams" }));
        } else if !running && was_running {
            debounce_counter += 1;
            log::debug!("teams_detection: Teams stopped, debounce {}/{}", debounce_counter, STOP_DEBOUNCE_POLLS);
            if debounce_counter >= STOP_DEBOUNCE_POLLS {
                TEAMS_PROCESS_WAS_RUNNING.store(false, Ordering::SeqCst);
                debounce_counter = 0;
                log::info!("teams_detection: Teams process stopped (debounced)");
                let _ = app.emit("teams-meeting-ended", serde_json::json!({}));
            }
        } else {
            if running {
                debounce_counter = 0;
            }
        }
    }
}
