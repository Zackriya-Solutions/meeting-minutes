use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};
use log::{info, warn};

use crate::audio::capture::linux_monitor::{is_monitor_available, DEFAULT_MONITOR_DEVICE_NAME};
use crate::audio::devices::configuration::{AudioDevice, DeviceType};

/// Configure Linux audio devices using ALSA/PulseAudio
pub fn configure_linux_audio(host: &cpal::Host) -> Result<Vec<AudioDevice>> {
    let mut devices = Vec::new();

    // Add input devices
    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            devices.push(AudioDevice::new(name, DeviceType::Input));
        }
    }

    // System audio.
    //
    // ALSA's device-name hints (which cpal enumerates) only expose PCM plugins
    // like `default`, `pulse` and `hw:...` - never the PulseAudio/PipeWire
    // monitor sources that actually carry system audio. Previously this scanned
    // cpal's input devices for names containing "monitor", which never matched,
    // so the UI listed no system audio devices at all.
    //
    // Instead we advertise a single logical device backed by
    // `pulse:@DEFAULT_MONITOR@`, which always tracks the current default sink.
    // See audio/capture/linux_monitor.rs.
    if is_monitor_available() {
        info!("🔊 [linux] System audio available via the default sink monitor");
        devices.push(AudioDevice::new(
            DEFAULT_MONITOR_DEVICE_NAME.to_string(),
            DeviceType::Output,
        ));
    } else {
        warn!("⚠️ [linux] No monitor source available - system audio cannot be captured");
        warn!("   Requires a running PulseAudio or PipeWire (pipewire-pulse) sound server");
    }

    Ok(devices)
}
