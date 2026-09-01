use std::sync::RwLock;
use tauri::{AppHandle, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewWindow};
use tauri_plugin_store::StoreExt;

const PREFERENCE_STORE: &str = "preferences.json";
const ENABLED_KEY: &str = "dictation_overlay_enabled";
const COMPACT_SIZE: (u32, u32) = (58, 52);
const EXPANDED_SIZE: (u32, u32) = (300, 94);
const BOTTOM_MARGIN: i32 = 16;

pub struct DictationOverlayState(RwLock<bool>);

impl DictationOverlayState {
    pub fn new() -> Self {
        Self(RwLock::new(true))
    }

    pub fn enabled(&self) -> bool {
        self.0.read().map(|value| *value).unwrap_or(true)
    }

    fn set_enabled(&self, enabled: bool) {
        if let Ok(mut value) = self.0.write() {
            *value = enabled;
        }
    }
}

impl Default for DictationOverlayState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn initialize_overlay(app: &AppHandle) {
    let enabled = app
        .store(PREFERENCE_STORE)
        .ok()
        .and_then(|store| store.get(ENABLED_KEY))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    app.state::<DictationOverlayState>().set_enabled(enabled);

    if enabled {
        if let Err(error) = resize_and_position(app, false) {
            log::warn!("dictation_overlay_initialize_failed error={error}");
        }
        show_if_enabled(app);
    } else if let Some(overlay) = app.get_webview_window("dictation-overlay") {
        let _ = overlay.hide();
    }
}

pub fn set_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let store = app
        .store(PREFERENCE_STORE)
        .map_err(|error| format!("Could not open overlay preferences: {error}"))?;
    store.set(ENABLED_KEY, serde_json::Value::Bool(enabled));
    store
        .save()
        .map_err(|error| format!("Could not save overlay preference: {error}"))?;
    app.state::<DictationOverlayState>().set_enabled(enabled);

    let overlay = overlay_window(app)?;
    if enabled {
        resize_and_position(app, false)?;
        overlay
            .show()
            .map_err(|error| format!("Could not show dictation overlay: {error}"))?;
    } else {
        overlay
            .hide()
            .map_err(|error| format!("Could not hide dictation overlay: {error}"))?;
    }
    log::info!("dictation_overlay_preference_changed enabled={enabled}");
    Ok(())
}

pub fn set_expanded(app: &AppHandle, expanded: bool) -> Result<(), String> {
    if !app.state::<DictationOverlayState>().enabled() {
        return Ok(());
    }
    resize_and_position(app, expanded)
}

pub fn prepare_for_activation(app: &AppHandle, focused_control_anchor: Option<(f64, f64)>) {
    if !app.state::<DictationOverlayState>().enabled() {
        return;
    }
    if let Err(error) = move_expanded_overlay_to_target_monitor(app, focused_control_anchor) {
        log::warn!("dictation_overlay_target_monitor_failed error={error}");
        if let Err(fallback_error) = resize_and_position(app, true) {
            log::warn!("dictation_overlay_activation_fallback_failed error={fallback_error}");
        }
    }
    show_if_enabled(app);
}

pub fn show_if_enabled(app: &AppHandle) {
    let Some(overlay) = app.get_webview_window("dictation-overlay") else {
        log::error!("dictation_overlay_missing code=internal");
        return;
    };
    if app.state::<DictationOverlayState>().enabled() {
        if let Err(error) = overlay.show() {
            log::warn!("dictation_overlay_show_failed error={error}");
        }
    } else if let Err(error) = overlay.hide() {
        log::warn!("dictation_overlay_hide_failed error={error}");
    }
}

fn resize_and_position(app: &AppHandle, expanded: bool) -> Result<(), String> {
    let overlay = overlay_window(app)?;
    let monitor = overlay
        .current_monitor()
        .map_err(|error| format!("Could not read overlay monitor: {error}"))?
        .ok_or_else(|| "No monitor is available for the dictation overlay.".to_string())?;
    resize_and_position_on_monitor(&overlay, &monitor, expanded)
}

fn move_expanded_overlay_to_target_monitor(
    app: &AppHandle,
    focused_control_anchor: Option<(f64, f64)>,
) -> Result<(), String> {
    let (anchor_x, anchor_y, source) = match focused_control_anchor {
        Some((x, y)) => (x, y, "focused_control"),
        None => {
            let cursor = app
                .cursor_position()
                .map_err(|error| format!("Could not read cursor position: {error}"))?;
            (cursor.x, cursor.y, "cursor_fallback")
        }
    };
    let monitor = app
        .available_monitors()
        .map_err(|error| format!("Could not enumerate monitors: {error}"))?
        .into_iter()
        .find(|monitor| {
            let position = monitor.position();
            let size = monitor.size();
            rectangle_contains_cursor(
                position.x,
                position.y,
                size.width,
                size.height,
                anchor_x,
                anchor_y,
            )
        })
        .ok_or_else(|| "The dictation target is not inside an available monitor.".to_string())?;
    let overlay = overlay_window(app)?;
    log::info!("dictation_overlay_monitor_selected source={source}");
    resize_and_position_on_monitor(&overlay, &monitor, true)
}

fn resize_and_position_on_monitor(
    overlay: &WebviewWindow,
    monitor: &Monitor,
    expanded: bool,
) -> Result<(), String> {
    let logical_size = if expanded {
        EXPANDED_SIZE
    } else {
        COMPACT_SIZE
    };
    let scale = monitor.scale_factor();
    let size = PhysicalSize::new(
        (logical_size.0 as f64 * scale).round() as u32,
        (logical_size.1 as f64 * scale).round() as u32,
    );
    overlay
        .set_size(size)
        .map_err(|error| format!("Could not resize dictation overlay: {error}"))?;
    position_for_monitor(overlay, monitor, size)
}

fn position_for_monitor(
    overlay: &WebviewWindow,
    monitor: &Monitor,
    size: PhysicalSize<u32>,
) -> Result<(), String> {
    let work_area = monitor.work_area();
    let scale = monitor.scale_factor();
    let bottom_margin = (BOTTOM_MARGIN as f64 * scale).round() as i32;
    let x = work_area.position.x + (work_area.size.width.saturating_sub(size.width) / 2) as i32;
    let y =
        work_area.position.y + work_area.size.height as i32 - size.height as i32 - bottom_margin;
    overlay
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|error| format!("Could not position dictation overlay: {error}"))
}

fn rectangle_contains_cursor(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    cursor_x: f64,
    cursor_y: f64,
) -> bool {
    cursor_x >= x as f64
        && cursor_x < x as f64 + width as f64
        && cursor_y >= y as f64
        && cursor_y < y as f64 + height as f64
}

fn overlay_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("dictation-overlay")
        .ok_or_else(|| "The dictation overlay window is unavailable.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_overlay_is_only_large_enough_for_the_voice_cursor() {
        assert_eq!(COMPACT_SIZE, (58, 52));
        assert!(EXPANDED_SIZE.0 > COMPACT_SIZE.0);
        assert!(EXPANDED_SIZE.1 > COMPACT_SIZE.1);
    }

    #[test]
    fn cursor_monitor_selection_supports_negative_desktop_coordinates() {
        assert!(rectangle_contains_cursor(
            -1920, -180, 1920, 1080, -640.0, 400.0
        ));
        assert!(!rectangle_contains_cursor(
            -1920, -180, 1920, 1080, 0.0, 400.0
        ));
        assert!(!rectangle_contains_cursor(
            -1920, -180, 1920, 1080, -640.0, 900.0
        ));
    }
}
