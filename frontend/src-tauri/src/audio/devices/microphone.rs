use anyhow::{anyhow, Result};
use cpal::traits::{HostTrait, DeviceTrait};

use super::configuration::{AudioDevice, DeviceType};

/// Get the default input (microphone) device for the system
pub fn default_input_device() -> Result<AudioDevice> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("No default input device found"))?;
    Ok(AudioDevice::new(device.name()?, DeviceType::Input))
}