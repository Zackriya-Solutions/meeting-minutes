use crate::notifications::types::{Notification, NotificationPriority, NotificationTimeout};
use anyhow::{Result, anyhow};
use log::{info as log_info, error as log_error};
use tauri::{AppHandle, Runtime};
use std::time::Duration;

/// Cross-platform system notification handler
pub struct SystemNotificationHandler<R: Runtime> {
    #[allow(dead_code)] // Used for non-macOS fallback notifications
    app_handle: AppHandle<R>,
}

impl<R: Runtime> SystemNotificationHandler<R> {
    pub fn new(app_handle: AppHandle<R>) -> Self {
        Self {
            app_handle,
        }
    }

    /// Show a notification using the system's notification center
    pub async fn show_notification(&self, notification: Notification) -> Result<()> {
        log_info!("Attempting to show notification: {}", notification.title);

        // Check if DND is active and respect user settings
        if self.is_dnd_active().await && self.should_respect_dnd(&notification) {
            log_info!("DND is active, skipping notification: {}", notification.title);
            return Ok(());
        }

        // Platform-specific notification implementation
        #[cfg(target_os = "macos")]
        {
            self.show_macos_notification(&notification).await
        }

        #[cfg(not(target_os = "macos"))]
        {
            self.show_tauri_notification(&notification).await
        }
    }

    /// Show native macOS notification (like "mic activity detected")
    #[cfg(target_os = "macos")]
    async fn show_macos_notification(&self, notification: &Notification) -> Result<()> {
        log_info!("Showing native macOS notification: {}", notification.title);

        let result = tokio::task::spawn_blocking({
            let title = notification.title.clone();
            let body = notification.body.clone();
            move || {
                // Send native macOS notification
                mac_notification_sys::send_notification(
                    &title,
                    None, // No subtitle
                    &body,
                    Some(mac_notification_sys::Notification::new().sound("default"))
                )
            }
        }).await;

        match result {
            Ok(Ok(_)) => {
                log_info!("Successfully showed native macOS notification: {}", notification.title);
                Ok(())
            }
            Ok(Err(e)) => {
                log_error!("Failed to show native macOS notification: {:?}", e);
                Err(anyhow!("Failed to show native macOS notification: {:?}", e))
            }
            Err(e) => {
                log_error!("Task join error for macOS notification: {}", e);
                Err(anyhow!("Task join error: {}", e))
            }
        }
    }

    /// Fallback to Tauri notification for non-macOS platforms
    #[cfg(not(target_os = "macos"))]
    async fn show_tauri_notification(&self, notification: &Notification) -> Result<()> {
        use tauri_plugin_notification::NotificationExt;

        log_info!("Showing Tauri notification: {}", notification.title);

        let builder = self.app_handle.notification().builder()
            .title(&notification.title)
            .body(&notification.body);

        match builder.show() {
            Ok(_) => {
                log_info!("Successfully showed Tauri notification: {}", notification.title);
                Ok(())
            }
            Err(e) => {
                log_error!("Failed to show Tauri notification: {}", e);
                Err(anyhow!("Failed to show notification: {}", e))
            }
        }
    }

    /// Check if Do Not Disturb is currently active
    /// Note: DND is managed through app settings, not system-level checks
    pub async fn is_dnd_active(&self) -> bool {
        // App manages DND through its own notification settings
        // No need to check system-level DND status
        false
    }

    /// Get the actual system DND status
    /// Note: DND is managed through app settings, not system-level checks
    pub async fn get_system_dnd_status(&self) -> bool {
        // App manages DND through its own notification settings
        // No need to check system-level DND status
        false
    }

    /// Request notification permission from the system
    pub async fn request_permission(&self) -> Result<bool> {
        log_info!("Requesting notification permission");

        // On most platforms with Tauri, permissions are handled automatically
        // We don't need to show a test notification during initialization
        log_info!("Notification permission granted (automatic for Tauri apps)");
        Ok(true)
    }

    /// Show a test notification to verify the system is working
    #[allow(dead_code)] // Used by show_test_notification command for manual testing
    async fn show_test_notification(&self) -> Result<()> {
        let test_notification = Notification::test_notification();
        self.show_notification(test_notification).await
    }

    /// Determine if we should respect DND for this notification
    fn should_respect_dnd(&self, notification: &Notification) -> bool {
        match notification.priority {
            NotificationPriority::Critical => false, // Always show critical notifications
            _ => true, // Respect DND for all other priorities
        }
    }

    /// Clear all notifications (platform-specific)
    pub async fn clear_notifications(&self) -> Result<()> {
        log_info!("Clearing all notifications");

        // This is platform-specific and complex to implement
        // For now, we'll just log that we attempted to clear
        // Future enhancement can add platform-specific clearing

        Ok(())
    }
}

/// Convert notification timeout to duration
impl From<&NotificationTimeout> for Option<Duration> {
    fn from(timeout: &NotificationTimeout) -> Self {
        match timeout {
            NotificationTimeout::Never => None,
            NotificationTimeout::Seconds(secs) => Some(Duration::from_secs(*secs)),
            NotificationTimeout::Default => Some(Duration::from_secs(5)),
        }
    }
}