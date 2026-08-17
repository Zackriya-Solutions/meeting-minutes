use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};

use crate::audio::devices::configuration::{AudioDevice, DeviceType};

/// Configure Linux audio devices using ALSA/PulseAudio
pub fn configure_linux_audio(host: &cpal::Host) -> Result<Vec<AudioDevice>> {
    let mut devices = Vec::new();

    // Add input devices. The virtual "meetily_system" ALSA device captures the
    // default output monitor; expose it only in the System (Output) slot so the
    // microphone and system audio can be recorded as separate, labeled streams.
    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            let device_type = if name.contains("meetily_system") {
                DeviceType::Output
            } else {
                DeviceType::Input
            };
            devices.push(AudioDevice::new(name, device_type));
        }
    }

    // Add PulseAudio monitor sources for system audio
    if let Ok(pulse_host) = cpal::host_from_id(cpal::HostId::Alsa) {
        for device in pulse_host.input_devices()? {
            if let Ok(name) = device.name() {
                // Check if it's a monitor source
                if name.contains("monitor") {
                    devices.push(AudioDevice::new(
                        format!("{} (System Audio)", name),
                        DeviceType::Output
                    ));
                }
            }
        }
    }

    Ok(devices)
}