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

    // Serialize every alsa-lib call on Linux: see ALSA_GLOBAL_LOCK doc comment
    // in devices/platform/linux.rs.
    #[cfg(target_os = "linux")]
    let _guard = platform::linux::ALSA_GLOBAL_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let host = cpal::default_host();
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
    let stream = match device.build_input_stream(
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