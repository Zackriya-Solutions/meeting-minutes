use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};
use log::info as log_info;
use std::path::PathBuf;
use tauri::{AppHandle, Runtime};
use dirs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettings {
    /// Enable recording lifecycle notifications (start/stop/pause/resume)
    pub recording_notifications: bool,

    /// Enable time-based meeting reminders
    pub time_based_reminders: bool,

    /// Enable meeting reminders based on calendar events
    pub meeting_reminders: bool,

    /// Respect system Do Not Disturb settings
    pub respect_do_not_disturb: bool,

    /// Enable notification sounds
    pub notification_sound: bool,

    /// System notification permission has been granted
    pub system_permission_granted: bool,

    /// User has completed the initial notification setup
    pub consent_given: bool,

    /// Manual DND mode (user-controlled)
    pub manual_dnd_mode: bool,

    /// Notification preferences for different types
    pub notification_preferences: NotificationPreferences,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPreferences {
    /// Show recording started notifications
    pub show_recording_started: bool,

    /// Show recording stopped notifications
    pub show_recording_stopped: bool,

    /// Show recording paused notifications
    pub show_recording_paused: bool,

    /// Show recording resumed notifications
    pub show_recording_resumed: bool,

    /// Show transcription complete notifications
    pub show_transcription_complete: bool,

    /// Show meeting reminder notifications
    pub show_meeting_reminders: bool,

    /// Show system error notifications
    pub show_system_errors: bool,

    /// Minutes before meeting to show reminder (0 = disabled)
    pub meeting_reminder_minutes: Vec<u64>,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            recording_notifications: true,
            time_based_reminders: true,
            meeting_reminders: true,
            respect_do_not_disturb: true,
            notification_sound: true,
            system_permission_granted: false,
            consent_given: false,
            manual_dnd_mode: false,
            notification_preferences: NotificationPreferences::default(),
        }
    }
}

impl Default for NotificationPreferences {
    fn default() -> Self {
        Self {
            show_recording_started: false,
            show_recording_stopped: false,
            show_recording_paused: true,
            show_recording_resumed: true,
            show_transcription_complete: true,
            show_meeting_reminders: true,
            show_system_errors: true,
            meeting_reminder_minutes: vec![15, 5], // 15 minutes and 5 minutes before
        }
    }
}

/// Manages notification consent and user preferences
pub struct ConsentManager<R: Runtime> {
    #[allow(dead_code)] // Reserved for future functionality
    app_handle: AppHandle<R>,
    settings_path: PathBuf,
}

impl<R: Runtime> ConsentManager<R> {
    pub fn new(app_handle: AppHandle<R>) -> Result<Self> {
        let settings_path = Self::get_settings_path()?;

        Ok(Self {
            app_handle,
            settings_path,
        })
    }

    /// Get the path where notification settings are stored
    fn get_settings_path() -> Result<PathBuf> {
        let mut path = dirs::config_dir()
            .ok_or_else(|| anyhow!("Could not find config directory"))?;

        path.push("meetily");
        path.push("notifications.json");

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        Ok(path)
    }

    /// Load notification settings from disk
    pub async fn load_settings(&self) -> Result<NotificationSettings> {
        if !self.settings_path.exists() {
            log_info!("No notification settings file found, using defaults");
            return Ok(NotificationSettings::default());
        }

        let content = tokio::fs::read_to_string(&self.settings_path).await?;
        let settings: NotificationSettings = serde_json::from_str(&content)?;

        log_info!("Loaded notification settings from disk");
        Ok(settings)
    }

    /// Save notification settings to disk
    pub async fn save_settings(&self, settings: &NotificationSettings) -> Result<()> {
        let content = serde_json::to_string_pretty(settings)?;
        tokio::fs::write(&self.settings_path, content).await?;

        log_info!("Saved notification settings to disk");
        Ok(())
    }

    /// Check if the user has given consent for notifications
    pub async fn has_consent(&self) -> bool {
        match self.load_settings().await {
            Ok(settings) => settings.consent_given,
            Err(_) => false,
        }
    }

    /// Check if system notification permission has been granted
    pub async fn has_system_permission(&self) -> bool {
        match self.load_settings().await {
            Ok(settings) => settings.system_permission_granted,
            Err(_) => false,
        }
    }

    /// Set user consent for notifications
    pub async fn set_consent(&self, consent: bool) -> Result<()> {
        let mut settings = self.load_settings().await.unwrap_or_default();
        settings.consent_given = consent;
        self.save_settings(&settings).await?;

        log_info!("Updated notification consent: {}", consent);
        Ok(())
    }

    /// Set system permission status
    pub async fn set_system_permission(&self, granted: bool) -> Result<()> {
        let mut settings = self.load_settings().await.unwrap_or_default();
        settings.system_permission_granted = granted;
        self.save_settings(&settings).await?;

        log_info!("Updated system notification permission: {}", granted);
        Ok(())
    }

    /// Update specific notification preferences
    pub async fn update_preferences(&self, preferences: NotificationPreferences) -> Result<()> {
        let mut settings = self.load_settings().await.unwrap_or_default();
        settings.notification_preferences = preferences;
        self.save_settings(&settings).await?;

        log_info!("Updated notification preferences");
        Ok(())
    }

    /// Enable or disable Do Not Disturb mode
    pub async fn set_dnd_mode(&self, enabled: bool) -> Result<()> {
        let mut settings = self.load_settings().await.unwrap_or_default();
        settings.manual_dnd_mode = enabled;
        self.save_settings(&settings).await?;

        log_info!("Set manual DND mode: {}", enabled);
        Ok(())
    }

    /// Check if notifications should be shown (considering consent, permissions, and DND)
    pub async fn should_show_notifications(&self) -> bool {
        match self.load_settings().await {
            Ok(settings) => {
                settings.consent_given
                    && settings.system_permission_granted
                    && !settings.manual_dnd_mode
            }
            Err(_) => false,
        }
    }

    /// Initialize notification settings on first app launch
    pub async fn initialize_on_first_launch(&self) -> Result<NotificationSettings> {
        if self.settings_path.exists() {
            return self.load_settings().await;
        }

        log_info!("First launch detected, initializing notification settings");
        let default_settings = NotificationSettings::default();
        self.save_settings(&default_settings).await?;

        Ok(default_settings)
    }

    /// Get settings with migration if needed
    pub async fn get_settings_with_migration(&self) -> Result<NotificationSettings> {
        let settings = self.load_settings().await.unwrap_or_default();

        // Perform any necessary migrations here
        // For example, if we add new settings in the future

        self.save_settings(&settings).await?;
        Ok(settings)
    }
}

/// Get default notification settings
pub fn get_default_settings() -> NotificationSettings {
    NotificationSettings::default()
}

/// Validate notification settings
pub fn validate_settings(settings: &NotificationSettings) -> Result<()> {
    // Validate meeting reminder minutes
    for &minutes in &settings.notification_preferences.meeting_reminder_minutes {
        if minutes > 1440 { // More than 24 hours
            return Err(anyhow!("Meeting reminder cannot be more than 24 hours (1440 minutes)"));
        }
    }

    Ok(())
}

/// Merge settings with defaults (for handling partial updates)
pub fn merge_with_defaults(partial: NotificationSettings) -> NotificationSettings {
    let _defaults = NotificationSettings::default();

    NotificationSettings {
        recording_notifications: partial.recording_notifications,
        time_based_reminders: partial.time_based_reminders,
        meeting_reminders: partial.meeting_reminders,
        respect_do_not_disturb: partial.respect_do_not_disturb,
        notification_sound: partial.notification_sound,
        system_permission_granted: partial.system_permission_granted,
        consent_given: partial.consent_given,
        manual_dnd_mode: partial.manual_dnd_mode,
        notification_preferences: partial.notification_preferences,
    }
}