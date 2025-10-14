use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use log::{debug, info, warn};

use crate::audio::devices::configuration::{AudioDevice, DeviceType};

/// Configure Windows audio devices using WASAPI
pub fn configure_windows_audio(host: &cpal::Host) -> Result<Vec<AudioDevice>> {
    let mut devices = Vec::new();

    // Get WASAPI devices
    if let Ok(wasapi_host) = cpal::host_from_id(cpal::HostId::Wasapi) {
        info!("Using WASAPI host for Windows audio device enumeration");

        // Add output devices (including loopback)
        if let Ok(output_devices) = wasapi_host.output_devices() {
            for device in output_devices {
                if let Ok(name) = device.name() {
                    // For Windows, we need to mark output devices specifically for loopback
                    // info!("Found Windows output device: {}", name);
                    devices.push(AudioDevice::new(name.clone(), DeviceType::Output));
                }
            }
        } else {
            warn!("Failed to enumerate WASAPI output devices");
        }

        // Add input devices from WASAPI
        if let Ok(input_devices) = wasapi_host.input_devices() {
            for device in input_devices {
                if let Ok(name) = device.name() {
                    // info!("Found Windows input device: {}", name);
                    devices.push(AudioDevice::new(name.clone(), DeviceType::Input));
                }
            }
        } else {
            warn!("Failed to enumerate WASAPI input devices");
        }
    } else {
        warn!("Failed to create WASAPI host, falling back to default host");
    }

    // If WASAPI failed or returned no devices, try default host as fallback
    if devices.is_empty() {
        debug!("WASAPI device enumeration failed or returned no devices, falling back to default host");
        // Add regular input devices
        if let Ok(input_devices) = host.input_devices() {
            for device in input_devices {
                if let Ok(name) = device.name() {
                    // info!("Found fallback input device: {}", name);
                    devices.push(AudioDevice::new(name.clone(), DeviceType::Input));
                }
            }
        } else {
            warn!("Failed to enumerate input devices from default host");
        }

        // Add output devices
        if let Ok(output_devices) = host.output_devices() {
            for device in output_devices {
                if let Ok(name) = device.name() {
                    // info!("Found fallback output device: {}", name);
                    devices.push(AudioDevice::new(name.clone(), DeviceType::Output));
                }
            }
        } else {
            warn!("Failed to enumerate output devices from default host");
        }
    }

    // If we still have no devices, add default devices
    if devices.is_empty() {
        warn!("No audio devices found, adding default devices only");

        // Try to add default input device
        if let Some(device) = host.default_input_device() {
            if let Ok(name) = device.name() {
                // info!("Adding default input device: {}", name);
                devices.push(AudioDevice::new(name, DeviceType::Input));
            }
        }

        // Try to add default output device
        if let Some(device) = host.default_output_device() {
            if let Ok(name) = device.name() {
                // info!("Adding default output device: {}", name);
                devices.push(AudioDevice::new(name, DeviceType::Output));
            }
        }
    }

    info!("Found {} Windows audio devices", devices.len());
    Ok(devices)
}

/// Get Windows device and configuration using WASAPI
pub fn get_windows_device(audio_device: &AudioDevice) -> Result<(cpal::Device, cpal::SupportedStreamConfig)> {
    let wasapi_host = cpal::host_from_id(cpal::HostId::Wasapi)
        .map_err(|e| anyhow!("Failed to create WASAPI host: {}", e))?;

    // Extract the base device name without the (input) or (output) suffix
    let base_name = if audio_device.name.ends_with(" (input)") {
        audio_device.name.trim_end_matches(" (input)")
    } else if audio_device.name.ends_with(" (output)") {
        audio_device.name.trim_end_matches(" (output)")
    } else {
        &audio_device.name
    };

    info!("Looking for Windows device with base name: {}", base_name);

    match audio_device.device_type {
        DeviceType::Input => {
            for device in wasapi_host.input_devices()? {
                if let Ok(name) = device.name() {
                    info!("Checking input device: {}", name);
                    // Check if the device name contains our base name
                    if name == base_name || name.contains(base_name) {
                        // info!("Found matching input device: {}", name);

                        // Try to get default input config with better error logging
                        match device.default_input_config() {
                            Ok(default_config) => {
                                // info!("Using default input config: {:?}", default_config);
                                return Ok((device, default_config));
                            },
                            Err(e) => {
                                warn!("Failed to get default input config: {}. Trying supported configs...", e);

                                // Try to find a supported configuration
                                if let Ok(supported_configs) = device.supported_input_configs() {
                                    let configs: Vec<_> = supported_configs.collect();
                                    if configs.is_empty() {
                                        warn!("No supported input configurations found for device: {}", name);
                                    } else {
                                        // info!("Found {} supported input configurations", configs.len());

                                        // First try to find F32 format with 2 channels (stereo)
                                        for config in &configs {
                                            if config.sample_format() == cpal::SampleFormat::F32 && config.channels() == 2 {
                                                let config = config.with_max_sample_rate();
                                                // info!("Using stereo F32 input config: {:?}", config);
                                                return Ok((device, config));
                                            }
                                        }

                                        // Then try any F32 format
                                        for config in &configs {
                                            if config.sample_format() == cpal::SampleFormat::F32 {
                                                let config = config.with_max_sample_rate();
                                                // info!("Using F32 input config: {:?}", config);
                                                return Ok((device, config));
                                            }
                                        }

                                        // Finally, use the first available config
                                        let config = configs[0].with_max_sample_rate();
                                        info!("Using fallback input config: {:?}", config);
                                        return Ok((device, config));
                                    }
                                } else {
                                    warn!("Could not enumerate supported configurations for device: {}", name);
                                }

                                return Err(anyhow!("No compatible input configuration found for device: {}", name));
                            }
                        }
                    }
                }
            }

            // If we didn't find a matching device, try the default input device as fallback
            info!("No matching input device found, trying default input device");
            if let Some(default_device) = wasapi_host.default_input_device() {
                if let Ok(_name) = default_device.name() {
                    // info!("Using default input device: {}", _name);
                    if let Ok(config) = default_device.default_input_config() {
                        return Ok((default_device, config));
                    } else if let Ok(supported_configs) = default_device.supported_input_configs() {
                        if let Some(config) = supported_configs.into_iter().next() {
                            return Ok((default_device, config.with_max_sample_rate()));
                        }
                    }
                }
            }
        }
        DeviceType::Output => {
            for device in wasapi_host.output_devices()? {
                if let Ok(name) = device.name() {
                    info!("Checking output device: {}", name);
                    // Check if the device name contains our base name
                    if name == base_name || name.contains(base_name) {
                        // info!("Found matching output device: {}", name);

                        // For output devices, we want to use them in loopback mode
                        if let Ok(supported_configs) = device.supported_output_configs() {
                            let configs: Vec<_> = supported_configs.collect();
                            if configs.is_empty() {
                                warn!("No supported output configurations found for device: {}", name);
                            } else {
                                // info!("Found {} supported output configurations", configs.len());

                                // Try to find a config that supports f32 format with 2 channels (stereo)
                                for config in &configs {
                                    if config.sample_format() == cpal::SampleFormat::F32 && config.channels() == 2 {
                                        let config = config.with_max_sample_rate();
                                        info!("Using stereo F32 output config: {:?}", config);
                                        return Ok((device, config));
                                    }
                                }

                                // Then try any F32 format
                                for config in &configs {
                                    if config.sample_format() == cpal::SampleFormat::F32 {
                                        let config = config.with_max_sample_rate();
                                        // info!("Using F32 output config: {:?}", config);
                                        return Ok((device, config));
                                    }
                                }

                                // Finally, use the first available config
                                let config = configs[0].with_max_sample_rate();
                                // info!("Using fallback output config: {:?}", config);
                                return Ok((device, config));
                            }
                        } else {
                            warn!("Could not enumerate supported configurations for device: {}", name);
                        }

                        // If we couldn't get supported configs, try default
                        if let Ok(default_config) = device.default_output_config() {
                            // info!("Using default output config: {:?}", default_config);
                            return Ok((device, default_config));
                        }
                    }
                }
            }

            // If we didn't find a matching device, try the default output device as fallback
            info!("No matching output device found, trying default output device");
            if let Some(default_device) = wasapi_host.default_output_device() {
                if let Ok(name) = default_device.name() {
                    info!("Using default output device: {}", name);
                    if let Ok(config) = default_device.default_output_config() {
                        return Ok((default_device, config));
                    } else if let Ok(supported_configs) = default_device.supported_output_configs() {
                        if let Some(config) = supported_configs.into_iter().next() {
                            return Ok((default_device, config.with_max_sample_rate()));
                        }
                    }
                }
            }
        }
    }

    Err(anyhow!("Device not found or no compatible configuration available: {}", audio_device.name))
}