use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};

use crate::audio::devices::configuration::{AudioDevice, DeviceType};

/// Configure Android audio devices using AAudio/OpenSL ES (cpal's oboe backend)
///
/// Android only exposes microphone input. Capturing other apps' audio is not
/// permitted by the platform, so no system (Output) devices are listed here —
/// meeting audio is picked up through the microphone instead.
pub fn configure_android_audio(host: &cpal::Host) -> Result<Vec<AudioDevice>> {
    let mut devices = Vec::new();

    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            devices.push(AudioDevice::new(name, DeviceType::Input));
        }
    }

    Ok(devices)
}
