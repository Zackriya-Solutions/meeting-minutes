use anyhow::{anyhow, Result};
use cidre::at::AudioBufListN;
use cidre::core_audio::hardware::System;
use cidre::os;
use log::debug;

use crate::audio::devices::configuration::{AudioDevice, DeviceType};

/// Whether a device exposes capture or playback channels in one scope.
///
/// Most devices answer for both scopes and report zero channels for the one
/// they do not serve, so an error here is unusual: a device in the middle of a
/// transition, or a virtual device that publishes its configuration late. Such
/// a device drops out of the list entirely, so log why rather than leaving the
/// user with a microphone that silently disappeared. `list_audio_devices` runs
/// every couple of seconds from the disconnect monitor, hence debug level.
fn scope_has_channels(cfg: os::Result<AudioBufListN>, device_name: &str, scope: &str) -> bool {
    match cfg {
        Ok(cfg) => cfg
            .buffers()
            .iter()
            .any(|buffer| buffer.number_channels > 0),
        Err(error) => {
            debug!(
                "Core Audio device '{device_name}' reported no {scope} stream configuration: {error}"
            );
            false
        }
    }
}

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
        let name = match device.name() {
            Ok(name) => name.to_string(),
            Err(error) => {
                debug!("Skipping Core Audio device {device:?} with no readable name: {error}");
                continue;
            }
        };

        if scope_has_channels(device.input_stream_cfg(), &name, "input") {
            devices.push(AudioDevice::new(name.clone(), DeviceType::Input));
        }

        if scope_has_channels(device.output_stream_cfg(), &name, "output")
            && should_include_output_device(&name)
        {
            devices.push(AudioDevice::new(name, DeviceType::Output));
        }
    }

    Ok(devices)
}
