use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};

use crate::audio::devices::configuration::{AudioDevice, DeviceType};

/// Configure macOS audio devices using ScreenCaptureKit and CoreAudio
pub fn configure_macos_audio(host: &cpal::Host) -> Result<Vec<AudioDevice>> {
    let mut devices: Vec<AudioDevice> = Vec::new();

    // Existing macOS implementation
    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            devices.push(AudioDevice::new(name, DeviceType::Input));
        }
    }

    // Filter function to exclude macOS speakers and AirPods for output devices
    fn should_include_output_device(name: &str) -> bool {
        !name.to_lowercase().contains("speakers") && !name.to_lowercase().contains("airpods")
    }

    if let Ok(host) = cpal::host_from_id(cpal::HostId::ScreenCaptureKit) {
        for device in host.input_devices()? {
            if let Ok(name) = device.name() {
                if should_include_output_device(&name) {
                    devices.push(AudioDevice::new(name, DeviceType::Output));
                }
            }
        }
    }

    for device in host.output_devices()? {
        if let Ok(name) = device.name() {
            if should_include_output_device(&name) {
                devices.push(AudioDevice::new(name, DeviceType::Output));
            }
        }
    }

    Ok(devices)
}