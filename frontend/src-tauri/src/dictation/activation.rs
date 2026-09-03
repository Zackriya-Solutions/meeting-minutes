use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::RwLock;
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

pub const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+Space";
const PREFERENCE_STORE: &str = "preferences.json";
const SHORTCUT_KEY: &str = "dictation_shortcut";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationShortcutStatus {
    pub enabled: bool,
    pub shortcut: Option<String>,
    pub message: Option<String>,
}

pub struct DictationShortcutStatusState(RwLock<DictationShortcutStatus>);

impl DictationShortcutStatusState {
    pub fn new() -> Self {
        Self(RwLock::new(DictationShortcutStatus {
            enabled: false,
            shortcut: None,
            message: Some("PulseTalq is registering a hold-to-talk shortcut.".into()),
        }))
    }

    pub fn registered(&self, shortcut: &str) {
        if let Ok(mut status) = self.0.write() {
            *status = DictationShortcutStatus {
                enabled: true,
                shortcut: Some(shortcut.to_owned()),
                message: None,
            };
        }
    }

    pub fn unavailable(&self) {
        if let Ok(mut status) = self.0.write() {
            *status = DictationShortcutStatus {
                enabled: false,
                shortcut: None,
                message: Some(
                    "All PulseTalq shortcut choices are currently used by other applications."
                        .into(),
                ),
            };
        }
    }

    pub fn get(&self) -> DictationShortcutStatus {
        self.0
            .read()
            .map(|status| status.clone())
            .unwrap_or(DictationShortcutStatus {
                enabled: false,
                shortcut: None,
                message: Some("Shortcut status is temporarily unavailable.".into()),
            })
    }
}

pub fn configured_shortcut<R: tauri::Runtime>(app: &AppHandle<R>) -> Option<String> {
    app.store(PREFERENCE_STORE)
        .ok()
        .and_then(|store| store.get(SHORTCUT_KEY))
        .and_then(|value| value.as_str().map(str::to_owned))
}

#[cfg(test)]
mod shortcut_config_tests {
    use super::DEFAULT_SHORTCUT;
    use std::str::FromStr;
    use tauri_plugin_global_shortcut::Shortcut;

    #[test]
    fn default_shortcut_is_accepted_by_global_shortcut_parser() {
        assert!(Shortcut::from_str(DEFAULT_SHORTCUT).is_ok());
    }
}

pub fn save_shortcut<R: tauri::Runtime>(app: &AppHandle<R>, shortcut: &str) -> Result<(), String> {
    let store = app
        .store(PREFERENCE_STORE)
        .map_err(|error| format!("Could not open dictation preferences: {error}"))?;
    store.set(SHORTCUT_KEY, serde_json::Value::String(shortcut.to_owned()));
    store
        .save()
        .map_err(|error| format!("Could not save dictation shortcut: {error}"))
}

impl Default for DictationShortcutStatusState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct KeyCode(pub u16);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldShortcut {
    keys: BTreeSet<KeyCode>,
}

impl HoldShortcut {
    pub fn new(keys: impl IntoIterator<Item = KeyCode>) -> Result<Self, ShortcutConfigError> {
        let keys: BTreeSet<_> = keys.into_iter().collect();
        if keys.is_empty() {
            return Err(ShortcutConfigError::Empty);
        }
        Ok(Self { keys })
    }

    pub fn keys(&self) -> &BTreeSet<KeyCode> {
        &self.keys
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutConfigError {
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationEvent {
    Started,
    Stopped,
}

pub struct ShortcutTracker {
    shortcut: HoldShortcut,
    pressed: BTreeSet<KeyCode>,
    active: bool,
}

impl ShortcutTracker {
    pub fn new(shortcut: HoldShortcut) -> Self {
        Self {
            shortcut,
            pressed: BTreeSet::new(),
            active: false,
        }
    }

    pub fn key_down(&mut self, key: KeyCode) -> Option<ActivationEvent> {
        self.pressed.insert(key);
        if !self.active && self.shortcut.keys.is_subset(&self.pressed) {
            self.active = true;
            return Some(ActivationEvent::Started);
        }
        None
    }

    pub fn key_up(&mut self, key: KeyCode) -> Option<ActivationEvent> {
        self.pressed.remove(&key);
        if self.active && self.shortcut.keys.contains(&key) {
            self.active = false;
            return Some(ActivationEvent::Stopped);
        }
        None
    }

    pub fn reset(&mut self) -> Option<ActivationEvent> {
        self.pressed.clear();
        if self.active {
            self.active = false;
            Some(ActivationEvent::Stopped)
        } else {
            None
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shortcut() -> HoldShortcut {
        HoldShortcut::new([KeyCode(0x11), KeyCode(0x20)]).unwrap()
    }

    #[test]
    fn starts_once_when_all_keys_are_held() {
        let mut tracker = ShortcutTracker::new(shortcut());

        assert_eq!(tracker.key_down(KeyCode(0x11)), None);
        assert_eq!(
            tracker.key_down(KeyCode(0x20)),
            Some(ActivationEvent::Started)
        );
        assert_eq!(tracker.key_down(KeyCode(0x20)), None);
        assert!(tracker.is_active());
    }

    #[test]
    fn releasing_any_shortcut_key_stops() {
        let mut tracker = ShortcutTracker::new(shortcut());
        tracker.key_down(KeyCode(0x11));
        tracker.key_down(KeyCode(0x20));

        assert_eq!(
            tracker.key_up(KeyCode(0x11)),
            Some(ActivationEvent::Stopped)
        );
        assert!(!tracker.is_active());
    }

    #[test]
    fn unrelated_keys_do_not_interrupt_dictation() {
        let mut tracker = ShortcutTracker::new(shortcut());
        tracker.key_down(KeyCode(0x11));
        tracker.key_down(KeyCode(0x20));

        assert_eq!(tracker.key_down(KeyCode(0x41)), None);
        assert_eq!(tracker.key_up(KeyCode(0x41)), None);
        assert!(tracker.is_active());
    }

    #[test]
    fn reset_stops_a_stuck_active_shortcut() {
        let mut tracker = ShortcutTracker::new(shortcut());
        tracker.key_down(KeyCode(0x11));
        tracker.key_down(KeyCode(0x20));

        assert_eq!(tracker.reset(), Some(ActivationEvent::Stopped));
        assert_eq!(tracker.reset(), None);
    }

    #[test]
    fn empty_shortcut_is_rejected() {
        assert_eq!(
            HoldShortcut::new([]).unwrap_err(),
            ShortcutConfigError::Empty
        );
    }
}
