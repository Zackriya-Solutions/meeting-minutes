use anyhow::Result;
use log::{info as log_info, error as log_error};

#[cfg(target_os = "macos")]
use mac_notification_sys::{send_notification, Notification};

pub struct EnhancedNotification {
    pub title: String,
    pub message: String,
    pub action_url: Option<String>,
    pub timeout_seconds: f64,
    pub action_button_text: String,
}

impl EnhancedNotification {
    pub fn new(
        title: impl Into<String>,
        message: impl Into<String>,
        action_button_text: impl Into<String>
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            action_url: None,
            timeout_seconds: 10.0,
            action_button_text: action_button_text.into(),
        }
    }

    pub fn with_action_url(mut self, url: impl Into<String>) -> Self {
        self.action_url = Some(url.into());
        self
    }

    pub fn with_timeout(mut self, seconds: f64) -> Self {
        self.timeout_seconds = seconds;
        self
    }

    pub fn recording_confirmation(meeting_name: Option<String>) -> Self {
        let title = "Start Recording";
        let message = if let Some(name) = meeting_name {
            format!("Ready to record meeting: {}", name)
        } else {
            "Ready to record your meeting - Click to confirm".to_string()
        };

        Self::new(title, message, "Start Recording")
            .with_timeout(15.0) // Longer timeout for user decision
    }
}

#[cfg(target_os = "macos")]
pub fn show_recording_confirmation(notification: &EnhancedNotification) -> Result<()> {
    log_info!("Showing enhanced recording confirmation notification");

    // Use the enhanced notification with subtitle and action information
    let body = format!("{}\n\nTap this notification to bring app to front", notification.message);

    let mut binding = Notification::new();
    let notif = binding
        .sound("Glass"); // Use a distinctive sound

    // Try to set subtitle if available
    let result = send_notification(
        &notification.title,
        Some(&notification.action_button_text),
        &body,
        Some(&notif)
    );

    match result {
        Ok(_) => {
            log_info!("Enhanced notification shown successfully");
            Ok(())
        }
        Err(e) => {
            log_error!("Failed to show enhanced notification: {:?}", e);
            Err(anyhow::anyhow!("Failed to show enhanced notification: {:?}", e))
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn show_recording_confirmation(_notification: &EnhancedNotification) -> Result<()> {
    log_info!("Enhanced notifications not supported on this platform");
    Err(anyhow::anyhow!("Enhanced notifications only supported on macOS"))
}

#[cfg(target_os = "macos")]
pub fn dismiss_all() -> Result<()> {
    // mac-notification-sys doesn't have a dismiss_all function, so this is a no-op
    log_info!("Dismiss all not supported with mac-notification-sys");
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn dismiss_all() -> Result<()> {
    log_info!("Enhanced notifications not supported on this platform");
    Ok(())
}

// Simplified handlers for cross-platform compatibility
pub fn setup_notification_confirm_handler<F>(_f: F)
where
    F: Fn(String) + Send + Sync + 'static,
{
    log_info!("Enhanced notification handlers are simplified for this implementation");
    // In this simplified version, we don't handle callbacks
    // The notification system will rely on the app becoming active when clicked
}

pub fn setup_notification_dismiss_handler<F>(_f: F)
where
    F: Fn(String) + Send + Sync + 'static,
{
    log_info!("Enhanced notification handlers are simplified for this implementation");
    // In this simplified version, we don't handle callbacks
    // The notification system will rely on the app becoming active when clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_creation() {
        let notification = EnhancedNotification::recording_confirmation(Some("Test Meeting".to_string()));
        assert_eq!(notification.title, "Start Recording");
        assert!(notification.message.contains("Test Meeting"));
        assert_eq!(notification.action_button_text, "Start Recording");
        assert_eq!(notification.timeout_seconds, 15.0);
    }

    #[test]
    fn test_notification_with_url() {
        let notification = EnhancedNotification::new("Title", "Message", "Action")
            .with_action_url("test://url")
            .with_timeout(5.0);

        assert_eq!(notification.action_url, Some("test://url".to_string()));
        assert_eq!(notification.timeout_seconds, 5.0);
    }
}