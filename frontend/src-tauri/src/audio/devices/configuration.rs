use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::atomic::AtomicU64;

lazy_static! {
    pub static ref LAST_AUDIO_CAPTURE: AtomicU64 = AtomicU64::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );
}

#[derive(Clone, Debug, PartialEq)]
pub enum AudioTranscriptionEngine {
    Deepgram,
    WhisperTiny,
    WhisperDistilLargeV3,
    WhisperLargeV3Turbo,
    WhisperLargeV3,
}

impl fmt::Display for AudioTranscriptionEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioTranscriptionEngine::Deepgram => write!(f, "Deepgram"),
            AudioTranscriptionEngine::WhisperTiny => write!(f, "WhisperTiny"),
            AudioTranscriptionEngine::WhisperDistilLargeV3 => write!(f, "WhisperLarge"),
            AudioTranscriptionEngine::WhisperLargeV3Turbo => write!(f, "WhisperLargeV3Turbo"),
            AudioTranscriptionEngine::WhisperLargeV3 => write!(f, "WhisperLargeV3"),
        }
    }
}

impl Default for AudioTranscriptionEngine {
    fn default() -> Self {
        AudioTranscriptionEngine::WhisperLargeV3Turbo
    }
}

#[derive(Clone, Debug)]
pub struct DeviceControl {
    pub is_running: bool,
    pub is_paused: bool,
}

#[derive(Clone, Eq, PartialEq, Hash, Serialize, Debug, Deserialize)]
pub enum DeviceType {
    Input,
    Output,
}

#[derive(Clone, Eq, PartialEq, Hash, Serialize, Debug)]
pub struct AudioDevice {
    pub name: String,
    pub device_type: DeviceType,
}

impl AudioDevice {
    pub fn new(name: String, device_type: DeviceType) -> Self {
        AudioDevice { name, device_type }
    }

    pub fn from_name(name: &str) -> Result<Self> {
        if name.trim().is_empty() {
            return Err(anyhow!("Device name cannot be empty"));
        }

        let (name, device_type) = if name.to_lowercase().ends_with("(input)") {
            (
                name.trim_end_matches("(input)").trim().to_string(),
                DeviceType::Input,
            )
        } else if name.to_lowercase().ends_with("(output)") {
            (
                name.trim_end_matches("(output)").trim().to_string(),
                DeviceType::Output,
            )
        } else {
            return Err(anyhow!(
                "Device type (input/output) not specified in the name"
            ));
        };

        Ok(AudioDevice::new(name, device_type))
    }
}

impl fmt::Display for AudioDevice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} ({})",
            self.name,
            match self.device_type {
                DeviceType::Input => "input",
                DeviceType::Output => "output",
            }
        )
    }
}

/// Parse audio device from string name
pub fn parse_audio_device(name: &str) -> Result<AudioDevice> {
    AudioDevice::from_name(name)
}

/// Get device and config for audio operations
pub async fn get_device_and_config(
    audio_device: &AudioDevice,
) -> Result<(cpal::Device, cpal::SupportedStreamConfig)> {
    #[cfg(target_os = "windows")]
    {
        return super::platform::get_windows_device(audio_device);
    }

    #[cfg(not(target_os = "windows"))]
    {
        use cpal::traits::{DeviceTrait, HostTrait};

        let host = cpal::default_host();
        debug!("[DeviceConfig] Looking for device: '{}' (type: {:?})", audio_device.name, audio_device.device_type);

        match audio_device.device_type {
            DeviceType::Input => {
                debug!("[DeviceConfig] Searching input devices for microphone...");
                for device in host.input_devices()? {
                    if let Ok(name) = device.name() {
                        debug!("[DeviceConfig] Checking input device: '{}'", name);
                        if name == audio_device.name {
                            let default_config = device
                                .default_input_config()
                                .map_err(|e| anyhow!("Failed to get default input config: {}", e))?;
                            info!("[DeviceConfig] Found microphone: '{}' with config: {:?}", name, default_config);
                            return Ok((device, default_config));
                        }
                    }
                }
            }
            DeviceType::Output => {
                #[cfg(target_os = "macos")]
                {
                    // Use default host for all macOS output devices
                    // Core Audio backend uses direct cidre API for system capture, not cpal
                    for device in host.output_devices()? {
                        if let Ok(name) = device.name() {
                            if name == audio_device.name {
                                let default_config = device
                                    .default_output_config()
                                    .map_err(|e| anyhow!("Failed to get output config: {}", e))?;
                                return Ok((device, default_config));
                            }
                        }
                    }
                }

                #[cfg(target_os = "linux")]
                {
                    // For Linux, system audio uses PulseAudio/PipeWire monitor sources
                    // Monitor sources are INPUT devices that capture audio from output sinks
                    info!("[DeviceConfig] Linux: Looking for system audio device '{}' in input devices", audio_device.name);

                    // Check if this is a PulseAudio/PipeWire source (contains "alsa_output" or ends with ".monitor")
                    let is_pulseaudio_source = audio_device.name.contains("alsa_output")
                        || audio_device.name.contains("alsa_input")
                        || audio_device.name.ends_with(".monitor");

                    if is_pulseaudio_source {
                        info!("[DeviceConfig] Linux: '{}' is a PulseAudio/PipeWire source", audio_device.name);

                        // For PulseAudio sources, we need to set PULSE_SOURCE and use "pulse" ALSA device
                        // This tells the pulse ALSA plugin which source to capture from
                        std::env::set_var("PULSE_SOURCE", &audio_device.name);
                        info!("[DeviceConfig] Linux: Set PULSE_SOURCE={}", audio_device.name);

                        // Find and use the "pulse" ALSA device
                        for device in host.input_devices()? {
                            if let Ok(name) = device.name() {
                                if name == "pulse" {
                                    let default_config = device
                                        .default_input_config()
                                        .map_err(|e| anyhow!("Failed to get pulse input config: {}", e))?;
                                    info!("[DeviceConfig] Linux: Using 'pulse' device to capture from '{}', config: {:?}",
                                          audio_device.name, default_config);
                                    return Ok((device, default_config));
                                }
                            }
                        }

                        warn!("[DeviceConfig] Linux: 'pulse' ALSA device not found - PulseAudio/ALSA plugin may not be installed");
                    }

                    // Collect all available input devices for debugging
                    let mut available_inputs: Vec<String> = Vec::new();

                    // Search input devices for the monitor source (ALSA direct)
                    for device in host.input_devices()? {
                        if let Ok(name) = device.name() {
                            available_inputs.push(name.clone());
                            debug!("[DeviceConfig] Linux: Checking input device: '{}'", name);

                            // Exact match
                            if name == audio_device.name {
                                let default_config = device
                                    .default_input_config()
                                    .map_err(|e| anyhow!("Failed to get default input config: {}", e))?;
                                info!("[DeviceConfig] Linux: Found exact match for system audio: '{}' with config: {:?}",
                                      name, default_config);
                                return Ok((device, default_config));
                            }
                        }
                    }

                    // Try partial matching (for friendly names or ALSA variants)
                    debug!("[DeviceConfig] Linux: No exact match, trying partial match...");
                    for device in host.input_devices()? {
                        if let Ok(name) = device.name() {
                            if name.contains(&audio_device.name) || audio_device.name.contains(&name) {
                                let default_config = device
                                    .default_input_config()
                                    .map_err(|e| anyhow!("Failed to get default input config: {}", e))?;
                                info!("[DeviceConfig] Linux: Found partial match for system audio: '{}' (requested: '{}')",
                                      name, audio_device.name);
                                return Ok((device, default_config));
                            }
                        }
                    }

                    // Log available devices for troubleshooting
                    warn!("[DeviceConfig] Linux: System audio device '{}' not found!", audio_device.name);
                    warn!("[DeviceConfig] Linux: Available input devices: {:?}", available_inputs);
                }
            }
        }

        Err(anyhow!("Device not found: {}", audio_device.name))
    }
}