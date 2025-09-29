use crate::notifications::types::{Notification, NotificationPriority, NotificationTimeout};
use anyhow::{Result, anyhow};
use log::{info as log_info, error as log_error, warn as log_warn};
use tauri::{AppHandle, Runtime};
use std::time::Duration;

#[cfg(not(target_os = "macos"))]
use tauri_plugin_notification::NotificationExt;

#[cfg(target_os = "macos")]
use std::process::Command;

#[cfg(target_os = "windows")]
use std::process::Command;

#[cfg(target_os = "linux")]
use std::process::Command;

/// Cross-platform system notification handler
pub struct SystemNotificationHandler<R: Runtime> {
    #[allow(dead_code)] // Used for non-macOS fallback notifications
    app_handle: AppHandle<R>,
    dnd_cache: std::sync::Arc<std::sync::Mutex<Option<(bool, std::time::Instant)>>>,
}

impl<R: Runtime> SystemNotificationHandler<R> {
    pub fn new(app_handle: AppHandle<R>) -> Self {
        Self {
            app_handle,
            dnd_cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
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

    /// Check if Do Not Disturb is currently active on the system
    pub async fn is_dnd_active(&self) -> bool {
        // Use cached value if it's fresh (less than 30 seconds old)
        if let Ok(cache) = self.dnd_cache.lock() {
            if let Some((cached_value, timestamp)) = *cache {
                if timestamp.elapsed() < Duration::from_secs(30) {
                    return cached_value;
                }
            }
        }

        let dnd_status = self.get_system_dnd_status().await;

        // Update cache
        if let Ok(mut cache) = self.dnd_cache.lock() {
            *cache = Some((dnd_status, std::time::Instant::now()));
        }

        dnd_status
    }

    /// Get the actual system DND status (platform-specific)
    pub async fn get_system_dnd_status(&self) -> bool {
        match self.check_platform_dnd().await {
            Ok(status) => status,
            Err(e) => {
                log_warn!("Failed to check DND status: {}, assuming false", e);
                false
            }
        }
    }

    /// Request notification permission from the system
    pub async fn request_permission(&self) -> Result<bool> {
        log_info!("Requesting notification permission");

        // On most platforms with Tauri, permissions are handled automatically
        // But we can still check if notifications are working
        match self.show_test_notification().await {
            Ok(_) => {
                log_info!("Notification permission appears to be granted");
                Ok(true)
            }
            Err(e) => {
                log_error!("Notification permission may be denied: {}", e);
                Ok(false)
            }
        }
    }

    /// Show a test notification to verify the system is working
    async fn show_test_notification(&self) -> Result<()> {
        let test_notification = Notification::test_notification();
        self.show_notification(test_notification).await
    }

    /// Platform-specific DND status checking
    async fn check_platform_dnd(&self) -> Result<bool> {
        #[cfg(target_os = "macos")]
        {
            self.check_macos_dnd().await
        }

        #[cfg(target_os = "windows")]
        {
            self.check_windows_dnd().await
        }

        #[cfg(target_os = "linux")]
        {
            self.check_linux_dnd().await
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            log_warn!("DND checking not implemented for this platform");
            Ok(false)
        }
    }

    #[cfg(target_os = "macos")]
    async fn check_macos_dnd(&self) -> Result<bool> {
        tokio::task::spawn_blocking(|| {
            let output = Command::new("defaults")
                .arg("read")
                .arg("com.apple.controlcenter")
                .arg("NSStatusItem Visible DoNotDisturb")
                .output();

            match output {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let is_dnd = stdout.trim() == "1";
                    log_info!("macOS DND status: {}", is_dnd);
                    Ok(is_dnd)
                }
                Err(e) => {
                    log_warn!("Failed to check macOS DND status: {}", e);
                    Ok(false)
                }
            }
        }).await.unwrap_or(Ok(false))
    }

    #[cfg(target_os = "windows")]
    async fn check_windows_dnd(&self) -> Result<bool> {
        tokio::task::spawn_blocking(|| {
            // Check Windows Focus Assist status
            let output = Command::new("powershell")
                .arg("-Command")
                .arg("Get-WinUserLanguageList | Select-Object -First 1 | Select-Object -ExpandProperty InputMethodTips")
                .output();

            match output {
                Ok(_) => {
                    // Windows DND detection is complex, for now return false
                    // This can be enhanced with Windows Registry checks
                    log_info!("Windows DND status check not fully implemented, assuming false");
                    Ok(false)
                }
                Err(e) => {
                    log_warn!("Failed to check Windows DND status: {}", e);
                    Ok(false)
                }
            }
        }).await.unwrap_or(Ok(false))
    }

    #[cfg(target_os = "linux")]
    async fn check_linux_dnd(&self) -> Result<bool> {
        tokio::task::spawn_blocking(|| {
            // Check GNOME DND status via gsettings
            let output = Command::new("gsettings")
                .arg("get")
                .arg("org.gnome.desktop.notifications")
                .arg("show-banners")
                .output();

            match output {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let show_banners = stdout.trim() == "true";
                    let is_dnd = !show_banners;
                    log_info!("Linux DND status: {}", is_dnd);
                    Ok(is_dnd)
                }
                Err(_) => {
                    // Try alternative method for KDE
                    let kde_output = Command::new("kreadconfig5")
                        .arg("--file")
                        .arg("plasmanotifyrc")
                        .arg("--group")
                        .arg("DoNotDisturb")
                        .arg("--key")
                        .arg("Enabled")
                        .output();

                    match kde_output {
                        Ok(output) => {
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            let is_dnd = stdout.trim() == "true";
                            log_info!("KDE DND status: {}", is_dnd);
                            Ok(is_dnd)
                        }
                        Err(e) => {
                            log_warn!("Failed to check Linux DND status: {}", e);
                            Ok(false)
                        }
                    }
                }
            }
        }).await.unwrap_or(Ok(false))
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