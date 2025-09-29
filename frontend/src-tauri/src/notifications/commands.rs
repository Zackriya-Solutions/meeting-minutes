use crate::notifications::{
    types::Notification,
    settings::NotificationSettings,
    manager::NotificationManager,
};

#[cfg(target_os = "macos")]
use crate::notifications::{
    EnhancedNotification, show_recording_confirmation, dismiss_all,
    setup_notification_confirm_handler, setup_notification_dismiss_handler,
};
use anyhow::Result;
use log::{info as log_info, error as log_error};
use tauri::{State, AppHandle, Runtime, Wry, Manager, Emitter};
use tauri_plugin_notification::NotificationExt;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared notification manager state
pub type NotificationManagerState<R> = Arc<RwLock<Option<NotificationManager<R>>>>;

/// Initialize the notification manager (called during app setup)
pub async fn initialize_notification_manager<R: Runtime>(
    app_handle: AppHandle<R>,
) -> Result<NotificationManager<R>> {
    log_info!("Initializing notification manager...");

    let manager = NotificationManager::new(app_handle).await?;
    manager.initialize().await?;

    log_info!("Notification manager initialized successfully");
    Ok(manager)
}

/// Get notification settings
#[tauri::command]
pub async fn get_notification_settings(
    manager_state: State<'_, NotificationManagerState<Wry>>
) -> Result<NotificationSettings, String> {
    log_info!("Getting notification settings");

    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        Ok(manager.get_settings().await)
    } else {
        Err("Notification manager not initialized".to_string())
    }
}

/// Set notification settings
#[tauri::command]
pub async fn set_notification_settings(
    settings: NotificationSettings,
    manager_state: State<'_, NotificationManagerState<Wry>>
) -> Result<(), String> {
    log_info!("Setting notification settings");

    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        manager.update_settings(settings).await
            .map_err(|e| format!("Failed to update settings: {}", e))
    } else {
        Err("Notification manager not initialized".to_string())
    }
}

/// Request notification permission from the system
#[tauri::command]
pub async fn request_notification_permission(
    manager_state: State<'_, NotificationManagerState<Wry>>
) -> Result<bool, String> {
    log_info!("Requesting notification permission");

    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        manager.request_permission().await
            .map_err(|e| format!("Failed to request permission: {}", e))
    } else {
        Err("Notification manager not initialized".to_string())
    }
}

/// Show a custom notification
#[tauri::command]
pub async fn show_notification(
    notification: Notification,
    manager_state: State<'_, NotificationManagerState<Wry>>
) -> Result<(), String> {
    log_info!("Showing custom notification: {}", notification.title);

    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        manager.show_notification(notification).await
            .map_err(|e| format!("Failed to show notification: {}", e))
    } else {
        Err("Notification manager not initialized".to_string())
    }
}

/// Show a test notification
#[tauri::command]
pub async fn show_test_notification(
    manager_state: State<'_, NotificationManagerState<Wry>>
) -> Result<(), String> {
    log_info!("Showing test notification");

    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        manager.show_test_notification().await
            .map_err(|e| format!("Failed to show test notification: {}", e))
    } else {
        Err("Notification manager not initialized".to_string())
    }
}

/// Check if Do Not Disturb is active
#[tauri::command]
pub async fn is_dnd_active(
    manager_state: State<'_, NotificationManagerState<Wry>>
) -> Result<bool, String> {
    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        Ok(manager.is_dnd_active().await)
    } else {
        Err("Notification manager not initialized".to_string())
    }
}

/// Get system Do Not Disturb status
#[tauri::command]
pub async fn get_system_dnd_status(
    manager_state: State<'_, NotificationManagerState<Wry>>
) -> Result<bool, String> {
    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        Ok(manager.get_system_dnd_status().await)
    } else {
        Err("Notification manager not initialized".to_string())
    }
}

/// Set manual Do Not Disturb mode
#[tauri::command]
pub async fn set_manual_dnd(
    enabled: bool,
    manager_state: State<'_, NotificationManagerState<Wry>>
) -> Result<(), String> {
    log_info!("Setting manual DND mode: {}", enabled);

    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        manager.set_manual_dnd(enabled).await
            .map_err(|e| format!("Failed to set manual DND: {}", e))
    } else {
        Err("Notification manager not initialized".to_string())
    }
}

/// Set user consent for notifications
#[tauri::command]
pub async fn set_notification_consent(
    consent: bool,
    manager_state: State<'_, NotificationManagerState<Wry>>
) -> Result<(), String> {
    log_info!("Setting notification consent: {}", consent);

    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        manager.set_consent(consent).await
            .map_err(|e| format!("Failed to set consent: {}", e))
    } else {
        Err("Notification manager not initialized".to_string())
    }
}

/// Clear all notifications
#[tauri::command]
pub async fn clear_notifications(
    manager_state: State<'_, NotificationManagerState<Wry>>
) -> Result<(), String> {
    log_info!("Clearing all notifications");

    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        manager.clear_notifications().await
            .map_err(|e| format!("Failed to clear notifications: {}", e))
    } else {
        Err("Notification manager not initialized".to_string())
    }
}

/// Check if notification system is ready
#[tauri::command]
pub async fn is_notification_system_ready(
    manager_state: State<'_, NotificationManagerState<Wry>>
) -> Result<bool, String> {
    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        Ok(manager.is_ready().await)
    } else {
        Ok(false)
    }
}

/// Initialize notification manager manually (for testing and ensuring it's ready)
#[tauri::command]
pub async fn initialize_notification_manager_manual(
    app: AppHandle<Wry>,
    manager_state: State<'_, NotificationManagerState<Wry>>
) -> Result<(), String> {
    log_info!("Manual initialization of notification manager requested");

    let manager_lock = manager_state.read().await;
    if manager_lock.is_some() {
        return Ok(()); // Already initialized
    }
    drop(manager_lock);

    // Initialize the manager
    match initialize_notification_manager(app).await {
        Ok(manager) => {
            let mut state = manager_state.write().await;
            *state = Some(manager);
            log_info!("Notification manager initialized successfully via manual command");
            Ok(())
        }
        Err(e) => {
            log_error!("Failed to initialize notification manager manually: {}", e);
            Err(format!("Failed to initialize notification manager: {}", e))
        }
    }
}

/// Test notification with automatic consent for development/testing
#[tauri::command]
pub async fn test_notification_with_auto_consent(
    app: AppHandle<Wry>,
    manager_state: State<'_, NotificationManagerState<Wry>>
) -> Result<(), String> {
    log_info!("Testing notification with automatic consent");

    // First ensure manager is initialized
    let manager_lock = manager_state.read().await;
    if manager_lock.is_none() {
        drop(manager_lock);
        if let Err(e) = initialize_notification_manager_manual(app.clone(), manager_state.clone()).await {
            return Err(format!("Failed to initialize manager: {}", e));
        }
    } else {
        drop(manager_lock);
    }

    // Get the manager again
    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        // Set consent and permissions automatically for testing
        if let Err(e) = manager.set_consent(true).await {
            log_error!("Failed to set consent: {}", e);
        }
        if let Err(e) = manager.request_permission().await {
            log_error!("Failed to request permission: {}", e);
        }

        // Show test notification
        manager.show_test_notification().await
            .map_err(|e| format!("Failed to show test notification: {}", e))
    } else {
        Err("Manager still not initialized".to_string())
    }
}

/// Get notification system statistics
#[tauri::command]
pub async fn get_notification_stats(
    manager_state: State<'_, NotificationManagerState<Wry>>
) -> Result<serde_json::Value, String> {
    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        let stats = manager.get_stats().await;
        serde_json::to_value(stats)
            .map_err(|e| format!("Failed to serialize stats: {}", e))
    } else {
        Err("Notification manager not initialized".to_string())
    }
}

// Helper functions for showing specific notification types
// These are used internally by the app and don't need to be Tauri commands

/// Show recording started notification (internal use)
pub async fn show_recording_started_notification<R: Runtime>(
    app_handle: &tauri::AppHandle<R>,
    manager_state: &NotificationManagerState<R>,
    meeting_name: Option<String>,
) -> Result<()> {
    log_info!("Attempting to show recording started notification for meeting: {:?}", meeting_name);

    // Check if manager is initialized
    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        log_info!("Notification manager found, showing recording started notification");

        // For testing/development, automatically grant consent and permissions
        if let Err(e) = manager.set_consent(true).await {
            log_error!("Failed to set consent: {}", e);
        }
        if let Err(e) = manager.request_permission().await {
            log_error!("Failed to request permission: {}", e);
        }

        manager.show_recording_started(meeting_name).await
    } else {
        drop(manager_lock);
        log_info!("Notification manager not initialized, initializing now...");

        // Try to initialize the manager first
        match initialize_notification_manager(app_handle.clone()).await {
            Ok(manager) => {
                // Store the manager in the state
                let mut state_lock = manager_state.write().await;
                *state_lock = Some(manager);
                drop(state_lock);

                log_info!("Notification manager initialized, showing notification...");

                // Now use the initialized manager
                let manager_lock = manager_state.read().await;
                if let Some(manager) = manager_lock.as_ref() {
                    // For testing/development, automatically grant consent and permissions
                    if let Err(e) = manager.set_consent(true).await {
                        log_error!("Failed to set consent: {}", e);
                    }
                    if let Err(e) = manager.request_permission().await {
                        log_error!("Failed to request permission: {}", e);
                    }
                    manager.show_recording_started(meeting_name).await
                } else {
                    log_error!("Manager still not available after initialization");
                    Ok(())
                }
            }
            Err(e) => {
                log_error!("Failed to initialize notification manager: {}", e);

                // Fallback: Use Tauri's notification API directly
                let title = "Meetily";
                let body = match meeting_name {
                    Some(name) => format!("Recording started for meeting: {}", name),
                    None => "Recording has started".to_string(),
                };

                log_info!("Using direct Tauri notification fallback: {} - {}", title, body);

                match app_handle.notification().builder()
                    .title(title)
                    .body(body)
                    .show()
                {
                    Ok(_) => {
                        log_info!("Successfully showed fallback notification: {}", title);
                        Ok(())
                    }
                    Err(e) => {
                        log_error!("Failed to show fallback notification: {}", e);
                        Err(anyhow::anyhow!("Failed to show notification: {}", e))
                    }
                }
            }
        }
    }
}

/// Show recording stopped notification (internal use)
pub async fn show_recording_stopped_notification<R: Runtime>(
    app_handle: &tauri::AppHandle<R>,
    manager_state: &NotificationManagerState<R>,
) -> Result<()> {
    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        manager.show_recording_stopped().await
    } else {
        drop(manager_lock);
        log_info!("Notification manager not initialized for stop notification, using fallback...");

        // Use direct Tauri notification as fallback for stop notification
        let title = "Meetily";
        let body = "Recording has stopped";

        log_info!("Using direct Tauri notification fallback: {} - {}", title, body);

        match app_handle.notification().builder()
            .title(title)
            .body(body)
            .show()
        {
            Ok(_) => {
                log_info!("Successfully showed fallback notification: {}", title);
                Ok(())
            }
            Err(e) => {
                log_error!("Failed to show fallback notification: {}", e);
                Err(anyhow::anyhow!("Failed to show notification: {}", e))
            }
        }
    }
}

/// Show recording paused notification (internal use)
pub async fn show_recording_paused_notification(
    manager_state: &NotificationManagerState<Wry>,
) -> Result<()> {
    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        manager.show_recording_paused().await
    } else {
        log_error!("Cannot show recording paused notification: manager not initialized");
        Ok(())
    }
}

/// Show recording resumed notification (internal use)
pub async fn show_recording_resumed_notification(
    manager_state: &NotificationManagerState<Wry>,
) -> Result<()> {
    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        manager.show_recording_resumed().await
    } else {
        log_error!("Cannot show recording resumed notification: manager not initialized");
        Ok(())
    }
}

/// Show transcription complete notification (internal use)
pub async fn show_transcription_complete_notification(
    manager_state: &NotificationManagerState<Wry>,
    file_path: Option<String>,
) -> Result<()> {
    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        manager.show_transcription_complete(file_path).await
    } else {
        log_error!("Cannot show transcription complete notification: manager not initialized");
        Ok(())
    }
}

/// Show system error notification (internal use)
pub async fn show_system_error_notification(
    manager_state: &NotificationManagerState<Wry>,
    error: String,
) -> Result<()> {
    let manager_lock = manager_state.read().await;
    if let Some(manager) = manager_lock.as_ref() {
        manager.show_system_error(error).await
    } else {
        log_error!("Cannot show system error notification: manager not initialized");
        Ok(())
    }
}

// Enhanced macOS notification commands

/// Show enhanced recording confirmation notification (macOS only)
#[cfg(target_os = "macos")]
#[tauri::command]
pub async fn show_enhanced_recording_confirmation(
    meeting_name: Option<String>,
    action_url: Option<String>,
) -> Result<(), String> {
    log_info!("Showing enhanced recording confirmation for meeting: {:?}", meeting_name);

    let notification = if let Some(url) = action_url {
        EnhancedNotification::recording_confirmation(meeting_name)
            .with_action_url(url)
    } else {
        EnhancedNotification::recording_confirmation(meeting_name)
    };

    show_recording_confirmation(&notification)
        .map_err(|e| format!("Failed to show enhanced notification: {}", e))
}

/// Show enhanced recording confirmation notification (non-macOS platforms)
#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub async fn show_enhanced_recording_confirmation(
    _meeting_name: Option<String>,
    _action_url: Option<String>,
) -> Result<(), String> {
    Err("Enhanced notifications are only supported on macOS".to_string())
}

/// Dismiss all enhanced notifications
#[tauri::command]
pub async fn dismiss_all_enhanced_notifications() -> Result<(), String> {
    log_info!("Dismissing all enhanced notifications");

    #[cfg(target_os = "macos")]
    {
        dismiss_all()
            .map_err(|e| format!("Failed to dismiss enhanced notifications: {}", e))
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(()) // No-op on non-macOS platforms
    }
}

/// Setup enhanced notification handlers (internal use)
#[cfg(target_os = "macos")]
pub fn setup_enhanced_notification_handlers(
    app_handle: AppHandle<Wry>,
) {
    log_info!("Setting up enhanced notification handlers");

    // Clone app handles for the closures
    let app_handle_confirm = app_handle.clone();
    let app_handle_dismiss = app_handle.clone();

    // Setup confirm handler
    setup_notification_confirm_handler(move |notification_id| {
        log_info!("Enhanced notification confirmed: {}", notification_id);

        // Focus the main window when notification is confirmed
        if let Some(window) = app_handle_confirm.get_webview_window("main") {
            if let Err(e) = window.set_focus() {
                log_error!("Failed to focus main window: {}", e);
            }
        }

        // Emit event to frontend to continue with recording
        if let Err(e) = app_handle_confirm.emit("enhanced-notification-confirmed", notification_id) {
            log_error!("Failed to emit enhanced notification confirmation event: {}", e);
        }
    });

    // Setup dismiss handler
    setup_notification_dismiss_handler(move |notification_id| {
        log_info!("Enhanced notification dismissed: {}", notification_id);

        // Emit event to frontend about dismissal
        if let Err(e) = app_handle_dismiss.emit("enhanced-notification-dismissed", notification_id) {
            log_error!("Failed to emit enhanced notification dismissal event: {}", e);
        }
    });
}

/// Helper function to show enhanced recording confirmation with app context
pub async fn show_enhanced_recording_confirmation_internal<R: Runtime>(
    _app_handle: &AppHandle<R>,
    meeting_name: Option<String>,
) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        log_info!("Showing enhanced recording confirmation notification internally");

        // Create the action URL to focus the app
        let action_url = "meetily://confirm-recording";

        let notification = EnhancedNotification::recording_confirmation(meeting_name)
            .with_action_url(action_url);

        show_recording_confirmation(&notification)
    }

    #[cfg(not(target_os = "macos"))]
    {
        log_info!("Enhanced notifications not supported on this platform, using fallback");
        // Fall back to regular notification for non-macOS
        let title = "Start Recording";
        let body = if let Some(name) = meeting_name {
            format!("Ready to record meeting: {}", name)
        } else {
            "Ready to record your meeting".to_string()
        };

        app_handle.notification().builder()
            .title(title)
            .body(body)
            .show()
            .map_err(|e| anyhow::anyhow!("Failed to show notification: {}", e))?;

        Ok(())
    }
}