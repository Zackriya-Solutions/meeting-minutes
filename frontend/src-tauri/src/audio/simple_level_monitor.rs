use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Runtime};
use anyhow::Result;
use log::{error, info};
use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct AudioLevelData {
    pub device_name: String,
    pub device_type: String, // "input" or "output"
    pub rms_level: f32,     // RMS level (0.0 to 1.0)
    pub peak_level: f32,    // Peak level (0.0 to 1.0)
    pub is_active: bool,    // Whether audio is being detected
}

#[derive(Debug, Serialize, Clone)]
pub struct AudioLevelUpdate {
    pub timestamp: u64,
    pub levels: Vec<AudioLevelData>,
}

// Simple global monitoring state
static IS_MONITORING: AtomicBool = AtomicBool::new(false);

/// Start audio level monitoring for specified devices
pub async fn start_monitoring<R: Runtime>(
    app_handle: AppHandle<R>,
    device_names: Vec<String>,
) -> Result<()> {
    info!("Starting simplified audio level monitoring for devices: {:?}", device_names);

    // Stop any existing monitoring
    IS_MONITORING.store(false, Ordering::SeqCst);

    // Wait a bit for any existing tasks to stop
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Start new monitoring
    IS_MONITORING.store(true, Ordering::SeqCst);

    // For now, create fake level data to test the UI
    let app_handle_clone = app_handle.clone();
    tokio::spawn(async move {
        let mut counter: f32 = 0.0;

        while IS_MONITORING.load(Ordering::SeqCst) {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            counter += 0.1;
            let fake_level = (counter.sin().abs() * 0.8) as f32; // Simulate varying levels

            let levels: Vec<AudioLevelData> = device_names.iter().map(|name| {
                AudioLevelData {
                    device_name: name.clone(),
                    device_type: "input".to_string(),
                    rms_level: fake_level,
                    peak_level: fake_level * 1.2,
                    is_active: fake_level > 0.1,
                }
            }).collect();

            let update = AudioLevelUpdate {
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                levels,
            };

            if let Err(e) = app_handle_clone.emit("audio-levels", &update) {
                error!("Failed to emit audio levels: {}", e);
                break;
            }
        }

        info!("Audio level monitoring task ended");
    });

    Ok(())
}

/// Stop audio level monitoring
pub async fn stop_monitoring() -> Result<()> {
    info!("Stopping simplified audio level monitoring");
    IS_MONITORING.store(false, Ordering::SeqCst);
    Ok(())
}

/// Check if currently monitoring
pub fn is_monitoring() -> bool {
    IS_MONITORING.load(Ordering::SeqCst)
}