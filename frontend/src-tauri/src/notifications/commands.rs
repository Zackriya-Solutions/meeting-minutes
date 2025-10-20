use crate::notifications::{
    types::Notification,
    settings::NotificationSettings,
    manager::NotificationManager,
};

use anyhow::Result;
use log::{info as log_info, error as log_error};
use tauri::{State, AppHandle, Runtime, Wry};
use tauri::Manager;
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
                    manager.show_recording_started(meeting_name).await
                } else {
                    log_error!("Manager still not available after initialization");
                    Ok(())
                }
            }
            Err(e) => {
                log_error!("Failed to initialize notification manager: {}", e);

                // Check settings before showing fallback notification
                use crate::notifications::settings::ConsentManager;
                let consent_manager = ConsentManager::new(app_handle.clone())?;
                let settings = consent_manager.load_settings().await.unwrap_or_default();

                if !settings.notification_preferences.show_recording_started {
                    log_info!("Recording started notification is disabled in settings, skipping fallback");
                    return Ok(());
                }

                // Fallback: Use Tauri's notification API directly
                let title = "Meetily";
                let body = match meeting_name {
                    Some(name) => format!("Recording started for meeting: {}", name),
                    None => "Recording has started. Please inform others in the meeting that you are recording.".to_string(),
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

        // Check settings before showing fallback notification
        use crate::notifications::settings::ConsentManager;
        let consent_manager = ConsentManager::new(app_handle.clone())?;
        let settings = consent_manager.load_settings().await.unwrap_or_default();

        if !settings.notification_preferences.show_recording_stopped {
            log_info!("Recording stopped notification is disabled in settings, skipping fallback");
            return Ok(());
        }

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