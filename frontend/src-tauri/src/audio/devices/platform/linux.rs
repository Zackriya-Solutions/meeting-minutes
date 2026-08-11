use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};
use log::{info, warn};

use crate::audio::capture::list_pulse_sinks;
use crate::audio::devices::configuration::{AudioDevice, DeviceType};

/// Configure Linux audio devices. Microphones still go through cpal/ALSA
/// (unaffected by this). System Audio entries come from PulseAudio/PipeWire's
/// own sink list (real descriptions, no ~/.asoundrc monitor-hint scanning) —
/// see audio/capture/pulse_linux.rs for the actual capture implementation.
pub fn configure_linux_audio(host: &cpal::Host) -> Result<Vec<AudioDevice>> {
    let mut devices = Vec::new();

    info!("🎙️ configure_linux_audio: enumerating cpal/ALSA input devices");
    // Add input devices
    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            devices.push(AudioDevice::new(name, DeviceType::Input));
        }
    }
    info!("🎙️ configure_linux_audio: cpal/ALSA gave {} input device(s), now listing Pulse sinks", devices.len());

    // Add PulseAudio/PipeWire sinks as "System Audio" devices
    match list_pulse_sinks() {
        Ok(sinks) => {
            info!("🎙️ configure_linux_audio: list_pulse_sinks returned {} sink(s)", sinks.len());
            for sink in sinks {
                devices.push(AudioDevice::new(
                    format!("{} (System Audio)", sink.description),
                    DeviceType::Output,
                ));
            }
        }
        Err(e) => {
            warn!("Failed to list PulseAudio/PipeWire sinks for System Audio: {}", e);
        }
    }

    Ok(devices)
}