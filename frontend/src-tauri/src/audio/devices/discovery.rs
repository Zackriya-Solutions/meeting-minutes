use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use log::{error, info};

use super::configuration::AudioDevice;
use super::platform;

/// List all available audio devices on the system
pub async fn list_audio_devices() -> Result<Vec<AudioDevice>> {
    info!("🎙️ list_audio_devices: start");
    let host = cpal::default_host();

    // Platform-specific device enumeration
    #[allow(unused_mut)]
    let mut devices = {
        #[cfg(target_os = "windows")]
        {
            platform::configure_windows_audio(&host)?
        }

        #[cfg(target_os = "linux")]
        {
            info!("🎙️ list_audio_devices: calling configure_linux_audio");
            let result = platform::configure_linux_audio(&host)?;
            info!("🎙️ list_audio_devices: configure_linux_audio returned {} device(s)", result.len());
            result
        }

        #[cfg(target_os = "macos")]
        {
            platform::configure_macos_audio(&host)?
        }
    };
    info!("🎙️ list_audio_devices: platform enumeration done, {} device(s) so far", devices.len());

    // Add any additional devices from the default host.
    //
    // On Linux this is intentionally disabled: configure_linux_audio() already
    // provides every useful device (PulseAudio/PipeWire sources and sinks, or a
    // filtered ALSA fallback). This block would otherwise re-inject all raw ALSA
    // PCM names as Output devices, polluting the "System Audio" picker with
    // entries like hdmi:, front:, sysdefault:, etc.
    #[cfg(not(target_os = "linux"))]
    {
        use super::configuration::DeviceType;

        if let Ok(other_devices) = host.devices() {
            for device in other_devices {
                if let Ok(name) = device.name() {
                    if !devices.iter().any(|d| d.name == name) {
                        devices.push(AudioDevice::new(name, DeviceType::Output));
                    }
                }
            }
        }
    }

    Ok(devices)
}

/// Trigger audio permission request on platforms that require it
/// Returns Ok(true) if permission is granted, Ok(false) if denied, Err if something went wrong
pub fn trigger_audio_permission() -> Result<bool> {
    use log::info;

    let host = cpal::default_host();

    // Serialize only the alsa-lib calls that touch the global config cache
    // (device/config lookup, PCM open via build_input_stream): see
    // ALSA_GLOBAL_LOCK doc comment in devices/platform/linux.rs. play()/sleep/
    // drop() below operate on an already-open handle and don't need the lock —
    // holding it that long would needlessly block every other Linux audio
    // caller (device_monitor's poll, a stream starting elsewhere) for the
    // ~500ms this function sleeps.
    let stream = {
        #[cfg(target_os = "linux")]
        let _guard = platform::linux::ALSA_GLOBAL_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let device = match host.default_input_device() {
            Some(d) => d,
            None => {
                info!("[trigger_audio_permission] No default input device found - permission likely denied");
                return Ok(false);
            }
        };

        let config = match device.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                info!("[trigger_audio_permission] Failed to get input config: {} - permission likely denied", e);
                return Ok(false);
            }
        };

        // Build and start an input stream to trigger the permission request
        match device.build_input_stream(
            &config.into(),
            |_data: &[f32], _: &cpal::InputCallbackInfo| {
                // Do nothing, we just want to trigger the permission request
            },
            |err| error!("Error in audio stream: {}", err),
            None,
        ) {
            Ok(s) => s,
            Err(e) => {
                info!("[trigger_audio_permission] Failed to build input stream: {} - permission likely denied", e);
                return Ok(false);
            }
        }
    };

    // Start the stream to actually trigger the permission dialog
    if let Err(e) = stream.play() {
        info!("[trigger_audio_permission] Failed to play stream: {} - permission likely denied", e);
        return Ok(false);
    }

    // Sleep briefly to allow the permission dialog to appear and for stream to actually work
    std::thread::sleep(std::time::Duration::from_millis(500));

    // If we got here, permission was granted
    info!("[trigger_audio_permission] Stream played successfully - permission granted");

    // Stop the stream
    drop(stream);

    Ok(true)
}

/// Verify the microphone is actually delivering audio (macOS permission check).
///
/// A macOS TCC denial can still let an input stream be *created*, but the audio
/// callback then never fires. This builds a brief test stream and waits for the
/// first callback: permission granted → fires in <100ms; denied → times out at
/// 5s and returns Err. Building + playing the stream also triggers the system
/// permission dialog if the state is notDetermined.
///
/// A denied mic fails the record-start path loudly instead of recording dead
/// audio. Deliberately minimal: it checks the default
/// input device and treats the first callback as "granted" (no sample-content
/// inspection, no per-selected-device check).
pub async fn verify_microphone_access() -> anyhow::Result<()> {
    use log::{info, warn};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    tokio::task::spawn_blocking(|| {
        let host = cpal::default_host();
        let Some(device) = host.default_input_device() else {
            // Let device resolution handle the existing system-audio-only fallback.
            info!("[verify_microphone_access] No microphone device found; skipping access check");
            return Ok(());
        };

        let config = device
            .default_input_config()
            .map_err(|e| anyhow::anyhow!("Failed to get microphone config: {}", e))?;

        let callback_fired = Arc::new(AtomicBool::new(false));
        let callback_fired_clone = callback_fired.clone();

        let stream = device
            .build_input_stream(
                &config.into(),
                move |_data: &[f32], _: &cpal::InputCallbackInfo| {
                    callback_fired_clone.store(true, Ordering::SeqCst);
                },
                |err| warn!("[verify_microphone_access] test stream error: {}", err),
                None,
            )
            .map_err(|e| anyhow::anyhow!("Cannot access microphone: {}", e))?;

        stream
            .play()
            .map_err(|e| anyhow::anyhow!("Cannot start microphone: {}", e))?;

        // Wait up to 5s for the first callback.
        for _ in 0..50 {
            if callback_fired.load(Ordering::SeqCst) {
                drop(stream);
                info!("[verify_microphone_access] microphone access verified");
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        drop(stream);
        Err(anyhow::anyhow!(
            "Microphone permission not granted. Please allow microphone access in \
             System Settings > Privacy & Security > Microphone, then try again."
        ))
    })
    .await
    .map_err(|e| anyhow::anyhow!("Microphone verification task failed: {}", e))?
}
