use anyhow::{anyhow, Result};
use cidre::core_audio::hardware::System;

use crate::audio::devices::configuration::{AudioDevice, DeviceType};

/// Configure macOS audio devices using ScreenCaptureKit and CoreAudio
///
/// CPAL's macOS `input_devices()` implementation probes every device by creating
/// a temporary AudioUnit. Doing that while a recording stream is starting (or
/// every two seconds from the disconnect monitor) can contend with the real
/// microphone AudioUnit and make Core Audio reconfigure the active input.
/// Core Audio's hardware property API gives us the same device list without
/// opening any capture streams.
pub fn configure_macos_audio(_host: &cpal::Host) -> Result<Vec<AudioDevice>> {
    let mut devices: Vec<AudioDevice> = Vec::new();

    // Filter function to exclude macOS built-in speakers for output devices
    // NOTE: AirPods and other Bluetooth devices are now allowed (with device monitoring for disconnect handling)
    fn should_include_output_device(name: &str) -> bool {
        // Only filter out built-in speakers (they don't typically capture system audio properly)
        !name.to_lowercase().contains("speakers")
    }

    let core_audio_devices = System::devices()
        .map_err(|error| anyhow!("Failed to enumerate Core Audio devices: {error}"))?;

    for device in core_audio_devices {
        let Ok(name) = device.name().map(|name| name.to_string()) else {
            continue;
        };

        let has_input = device
            .input_stream_cfg()
            .map(|config| {
                config
                    .buffers()
                    .iter()
                    .any(|buffer| buffer.number_channels > 0)
            })
            .unwrap_or(false);
        if has_input {
            devices.push(AudioDevice::new(name.clone(), DeviceType::Input));
        }

        let has_output = device
            .output_stream_cfg()
            .map(|config| {
                config
                    .buffers()
                    .iter()
                    .any(|buffer| buffer.number_channels > 0)
            })
            .unwrap_or(false);
        if has_output && should_include_output_device(&name) {
            devices.push(AudioDevice::new(name, DeviceType::Output));
        }
    }

    Ok(devices)
}
