// macOS audio permissions handling
use anyhow::Result;
use log::{info, warn, error};

#[cfg(target_os = "macos")]
use std::process::Command;

/// Check if the app has Screen Recording permission (required for system audio capture)
#[cfg(target_os = "macos")]
pub fn check_screen_recording_permission() -> bool {
    // Use a simple approach: try to access the default output device
    // This will fail if we don't have system audio recording permission
    use cidre::core_audio as ca;
    
    match ca::System::default_output_device() {
        Ok(_) => {
            info!("✅ System audio recording permission granted");
            true
        }
        Err(e) => {
            // Check if this is a permission error
            let error_msg = e.to_string().to_lowercase();
            if error_msg.contains("permission") || error_msg.contains("access") {
                warn!("⚠️  System audio recording permission NOT granted: {}", e);
                false
            } else {
                // If it's not a permission error, assume permission is granted but there's another issue
                warn!("⚠️  System audio recording permission check failed (non-permission error): {}", e);
                true // Assume permission is granted if it's not a permission error
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn check_screen_recording_permission() -> bool {
    true // Not required on other platforms
}

/// Request Screen Recording permission from the user
/// This will open System Settings to the Screen Recording permission page
#[cfg(target_os = "macos")]
pub fn request_screen_recording_permission() -> Result<()> {
    info!("🔐 Requesting Screen Recording permission...");

    // Open System Settings to Screen Recording page
    let result = Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        .spawn();

    match result {
        Ok(_) => {
            info!("✅ Opened System Settings for Screen Recording permission");
            info!("👉 Please enable Screen Recording permission and restart the app");
            Ok(())
        }
        Err(e) => {
            error!("❌ Failed to open System Settings: {}", e);
            Err(anyhow::anyhow!("Failed to open System Settings: {}", e))
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn request_screen_recording_permission() -> Result<()> {
    Ok(()) // Not required on other platforms
}

/// Check and request Screen Recording permission if not granted
/// Returns true if permission is granted, false otherwise
pub fn ensure_screen_recording_permission() -> bool {
    if check_screen_recording_permission() {
        return true;
    }

    warn!("Screen Recording permission not granted - requesting...");

    if let Err(e) = request_screen_recording_permission() {
        error!("Failed to request Screen Recording permission: {}", e);
        return false;
    }

    false // Permission will be granted after restart
}

/// Tauri command to check Screen Recording permission
#[tauri::command]
pub async fn check_screen_recording_permission_command() -> bool {
    check_screen_recording_permission()
}

/// Tauri command to request Screen Recording permission
#[tauri::command]
pub async fn request_screen_recording_permission_command() -> Result<(), String> {
    request_screen_recording_permission()
        .map_err(|e| e.to_string())
}

/// Trigger system audio permission request programmatically
/// This attempts to create a system audio stream to trigger the permission dialog
#[cfg(target_os = "macos")]
pub fn trigger_system_audio_permission() -> Result<()> {
    info!("🔐 Triggering system audio permission request...");
    
    // Try to create a Core Audio capture and stream to trigger the permission dialog
    // This will fail if permission is not granted, but will trigger the dialog
    match crate::audio::capture::CoreAudioCapture::new() {
        Ok(capture) => {
            info!("✅ Core Audio capture created, attempting to create stream...");
            
            // Try to create a stream - this is what actually triggers the permission dialog
            match capture.stream() {
                Ok(_stream) => {
                    info!("✅ System audio permission already granted - stream created successfully");
                    Ok(())
                }
                Err(e) => {
                    // Check if this is a permission error
                    let error_msg = e.to_string().to_lowercase();
                    if error_msg.contains("permission") || error_msg.contains("screen recording") {
                        info!("🔐 System audio permission dialog should have appeared");
                        info!("👉 Please grant Screen Recording permission and restart the app");
                        Ok(()) // This is expected - we triggered the dialog
                    } else {
                        warn!("⚠️ Failed to create system audio stream: {}", e);
                        Err(e)
                    }
                }
            }
        }
        Err(e) => {
            // Check if this is a permission error
            let error_msg = e.to_string().to_lowercase();
            if error_msg.contains("permission") || error_msg.contains("screen recording") {
                info!("🔐 System audio permission dialog should have appeared");
                info!("👉 Please grant Screen Recording permission and restart the app");
                Ok(()) // This is expected - we triggered the dialog
            } else {
                warn!("⚠️ Failed to trigger system audio permission: {}", e);
                Err(e)
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn trigger_system_audio_permission() -> Result<()> {
    // System audio permissions not required on other platforms
    info!("System audio permissions not required on this platform");
    Ok(())
}

/// Tauri command to trigger system audio permission request
#[tauri::command]
pub async fn trigger_system_audio_permission_command() -> Result<(), String> {
    trigger_system_audio_permission()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_permission() {
        let has_permission = check_screen_recording_permission();
        println!("Has Screen Recording permission: {}", has_permission);
    }
}