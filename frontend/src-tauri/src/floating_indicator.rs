//! Floating recording indicator.
//!
//! A small always-on-top, draggable pill window that appears while a
//! recording is active and the main window is not focused. It shows a live
//! microphone waveform, the elapsed time, and a stop button.
//!
//! Visibility and level updates are driven by a single background task so
//! every start/stop path (UI, tray, errors) is handled uniformly.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

pub const WINDOW_LABEL: &str = "floating-indicator";

const PILL_WIDTH: f64 = 124.0;
const PILL_HEIGHT: f64 = 30.0;
const TICK_MS: u64 = 150;

/// Preference store file and key, shared with the frontend settings UI.
const PREFERENCES_STORE: &str = "preferences.json";
const ENABLED_KEY: &str = "show_floating_indicator";

/// Whether the floating indicator feature is enabled (user setting). Defaults to
/// on; the persisted value is loaded at startup and updated live from settings.
static ENABLED: AtomicBool = AtomicBool::new(true);

/// Whether the main window currently has focus (updated from `on_window_event`).
static MAIN_FOCUSED: AtomicBool = AtomicBool::new(true);

/// Whether the pill is currently being dragged. While dragging, macOS briefly
/// activates the app (focusing the main window), which would otherwise make the
/// driver hide the pill mid-drag. This flag suppresses that hide.
static DRAGGING: AtomicBool = AtomicBool::new(false);

pub fn set_main_focused(focused: bool) {
    MAIN_FOCUSED.store(focused, Ordering::SeqCst);
}

/// Set from the indicator UI while a window drag is in progress.
#[tauri::command]
pub fn set_indicator_dragging(dragging: bool) {
    DRAGGING.store(dragging, Ordering::SeqCst);
}

/// Enable/disable the floating indicator live from the settings UI.
#[tauri::command]
pub fn set_floating_indicator_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
}

/// Load the persisted enabled preference (defaults to true if unset/unreadable).
fn load_enabled_preference<R: Runtime>(app: &AppHandle<R>) {
    use tauri_plugin_store::StoreExt;
    if let Ok(store) = app.store(PREFERENCES_STORE) {
        if let Some(value) = store.get(ENABLED_KEY) {
            if let Some(enabled) = value.as_bool() {
                ENABLED.store(enabled, Ordering::SeqCst);
            }
        }
    }
}

/// Decide whether the pill should be visible.
///
/// Visible only while recording and enabled. It stays up during a drag (macOS
/// focuses the main window while the OS drag loop runs), and otherwise only when
/// neither the main window nor the pill itself is focused.
fn should_show_pill(
    recording: bool,
    enabled: bool,
    dragging: bool,
    main_focused: bool,
    pill_focused: bool,
) -> bool {
    recording && enabled && (dragging || (!main_focused && !pill_focused))
}

/// Spawn the background task that drives the indicator window.
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    load_enabled_preference(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut visible = false;
        let mut interval = tokio::time::interval(Duration::from_millis(TICK_MS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;

            let recording = crate::audio::recording_commands::is_recording().await;

            if recording {
                let state = crate::audio::recording_commands::get_recording_state().await;
                let (rms, peak) = crate::audio::mic_level::get();
                let payload = serde_json::json!({
                    "rms": rms,
                    "peak": peak,
                    "is_paused": state["is_paused"].as_bool().unwrap_or(false),
                    "duration": state["recording_duration"].as_f64(),
                });
                let _ = app.emit("mic-level", payload);
            } else {
                crate::audio::mic_level::reset();
                DRAGGING.store(false, Ordering::SeqCst);
            }

            // Query the main window's focus directly each tick. This is more
            // reliable than caching focus events, which don't always fire for a
            // borderless accessory window on macOS.
            let main_focused = app
                .get_webview_window("main")
                .and_then(|w| w.is_focused().ok())
                .unwrap_or_else(|| MAIN_FOCUSED.load(Ordering::SeqCst));

            // Keep the pill visible while the user is interacting with it.
            let pill_focused = app
                .get_webview_window(WINDOW_LABEL)
                .and_then(|w| w.is_focused().ok())
                .unwrap_or(false);

            // Keep the pill visible throughout a drag, even though macOS focuses
            // the main window while the OS drag loop is active.
            let dragging = DRAGGING.load(Ordering::SeqCst);

            let enabled = ENABLED.load(Ordering::SeqCst);

            let should_show =
                should_show_pill(recording, enabled, dragging, main_focused, pill_focused);
            if should_show != visible {
                if should_show {
                    show_indicator(&app);
                } else {
                    hide_indicator(&app);
                }
                visible = should_show;
            }
        }
    });
}

fn show_indicator<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = ensure_window(app) {
        if let Err(e) = window.show() {
            log::error!("Floating indicator: failed to show window: {}", e);
        }
        let _ = window.set_always_on_top(true);
    }
}

fn hide_indicator<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        if let Err(e) = window.hide() {
            log::error!("Floating indicator: failed to hide window: {}", e);
        }
    }
}

/// Get the indicator window, creating it (hidden) on first use.
/// The window is reused across recordings so it keeps its dragged position.
fn ensure_window<R: Runtime>(app: &AppHandle<R>) -> Option<tauri::WebviewWindow<R>> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        return Some(window);
    }

    let mut builder = WebviewWindowBuilder::new(
        app,
        WINDOW_LABEL,
        WebviewUrl::App("floating-indicator.html".into()),
    )
    .title("Recording")
    .inner_size(PILL_WIDTH, PILL_HEIGHT)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .visible_on_all_workspaces(true)
    .skip_taskbar(true)
    .accept_first_mouse(true)
    // Non-focusable: clicking the pill must not activate the app and raise the
    // main window. Combined with accept_first_mouse, the webview still receives
    // the click so the stop button and window dragging keep working.
    .focusable(false)
    .focused(false)
    .visible(false);

    // Default position: top-right corner of the primary monitor
    if let Ok(Some(monitor)) = app.primary_monitor() {
        let scale = monitor.scale_factor();
        let size = monitor.size().to_logical::<f64>(scale);
        let pos = monitor.position().to_logical::<f64>(scale);
        builder = builder.position(pos.x + size.width - PILL_WIDTH - 24.0, pos.y + 48.0);
    }

    match builder.build() {
        Ok(window) => Some(window),
        Err(e) => {
            log::error!("Floating indicator: failed to create window: {}", e);
            None
        }
    }
}

/// Stop the active recording from the floating indicator.
/// Mirrors the tray stop flow: generates a save path, stops the recording and
/// notifies the frontend so post-processing (DB save, navigation) runs there.
#[tauri::command]
pub async fn stop_recording_from_indicator<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    log::info!("Floating indicator: stop recording requested");

    if !crate::audio::recording_commands::is_recording().await {
        hide_indicator(&app);
        return Ok(());
    }

    hide_indicator(&app);
    crate::tray::set_tray_state(&app, crate::tray::RecordingState::Stopping);
    crate::tray::focus_main_window(&app);

    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    let timestamp = chrono::Local::now().format("%Y-%m-%dT%H-%M-%S").to_string();
    let save_path = data_dir.join(format!("recording-{}.wav", timestamp));

    crate::audio::recording_commands::stop_recording(
        app.clone(),
        crate::audio::recording_commands::RecordingArgs {
            save_path: save_path.to_string_lossy().to_string(),
        },
    )
    .await?;

    // Trigger frontend post-processing (SQLite save, navigation, analytics)
    if let Err(e) = app.emit("recording-stop-complete", true) {
        log::error!("Floating indicator: failed to emit recording-stop-complete: {}", e);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::should_show_pill;

    #[test]
    fn hidden_when_not_recording() {
        // Nothing shows unless a recording is active, regardless of focus/drag.
        assert!(!should_show_pill(false, true, false, false, false));
        assert!(!should_show_pill(false, true, true, false, false));
    }

    #[test]
    fn hidden_when_disabled() {
        assert!(!should_show_pill(true, false, false, false, false));
        assert!(!should_show_pill(true, false, true, false, false));
    }

    #[test]
    fn shown_when_recording_and_main_unfocused() {
        assert!(should_show_pill(true, true, false, false, false));
    }

    #[test]
    fn hidden_when_main_window_focused() {
        // Returning to the main window hides the pill.
        assert!(!should_show_pill(true, true, false, true, false));
    }

    #[test]
    fn hidden_when_pill_focused_but_not_dragging() {
        assert!(!should_show_pill(true, true, false, false, true));
    }

    #[test]
    fn stays_visible_while_dragging_even_if_main_focused() {
        // macOS focuses the main window during the OS drag loop; the pill must
        // stay visible throughout the drag.
        assert!(should_show_pill(true, true, true, true, false));
        assert!(should_show_pill(true, true, true, true, true));
    }
}
