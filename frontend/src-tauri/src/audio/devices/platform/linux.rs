use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};

use crate::audio::capture::pipewire;
use crate::audio::devices::configuration::{AudioDevice, DeviceType};

/// Configure Linux audio devices: ALSA inputs for microphones, PipeWire for system audio.
///
/// ALSA does not expose PulseAudio/PipeWire monitor sources, so system audio is offered
/// as a single PipeWire device that records the default sink's monitor.
pub fn configure_linux_audio(host: &cpal::Host) -> Result<Vec<AudioDevice>> {
    let mut devices = Vec::new();

    // Add input devices
    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            devices.push(AudioDevice::new(name, DeviceType::Input));
        }
    }

    if pipewire::is_available() {
        devices.push(AudioDevice::new(
            pipewire::PIPEWIRE_SYSTEM_DEVICE.to_string(),
            DeviceType::Output,
        ));
    }

    Ok(devices)
}