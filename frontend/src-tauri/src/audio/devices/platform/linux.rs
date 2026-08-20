use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};
use log::warn;

use crate::audio::capture::pulse;
use crate::audio::devices::configuration::{AudioDevice, DeviceType};

/// Configure Linux audio devices.
///
/// Microphones are enumerated through cpal's ALSA backend (works
/// correctly: real capture hardware is exposed as ALSA PCM devices).
/// System audio has no ALSA equivalent -- PulseAudio/PipeWire monitor
/// sources are only visible through the PulseAudio client protocol, so
/// they're listed via `pactl` instead (see `audio::capture::pulse`).
pub fn configure_linux_audio(host: &cpal::Host) -> Result<Vec<AudioDevice>> {
    let mut devices = Vec::new();

    // Add input devices (microphones)
    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            devices.push(AudioDevice::new(name, DeviceType::Input));
        }
    }

    // Add PulseAudio/PipeWire monitor sources for system audio capture
    match pulse::list_monitor_sources() {
        Ok(sources) => {
            for source in sources {
                devices.push(AudioDevice::new(source.description, DeviceType::Output));
            }
        }
        Err(e) => {
            warn!("No system audio monitor sources available: {}", e);
        }
    }

    Ok(devices)
}