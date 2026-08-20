// dictation/types.rs
//
// Shared state and settings types for the live dictation feature (issue #719).

use serde::{Deserialize, Serialize};

/// Current lifecycle state of the dictation feature, surfaced to the frontend
/// (tray tooltip / settings UI) so the user always has a persistent indicator
/// of whether dictation is listening.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictationState {
    /// Dictation is off; no AT-SPI connection is held.
    Idle,
    /// Dictation is on and actively injecting transcribed segments.
    Listening,
    /// Dictation is on, but the most recent segment could not be injected
    /// (no focused editable field, a password field, or an AT-SPI error) and
    /// was routed to the clipboard fallback instead.
    InjectFailedFallback,
}

impl Default for DictationState {
    fn default() -> Self {
        DictationState::Idle
    }
}

/// User-configurable dictation preferences, persisted via `tauri-plugin-store`
/// (see `dictation::settings`, mirroring `audio::recording_preferences`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictationSettings {
    /// Whether live dictation is enabled at all.
    pub enabled: bool,
    /// Global hotkey string (e.g. "Alt+Shift+D") used to toggle dictation on/off.
    /// `None` means no global hotkey is registered; the tray toggle still works.
    pub hotkey: Option<String>,
}

impl Default for DictationSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            hotkey: Some("Alt+Shift+D".to_string()),
        }
    }
}
