use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
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

/// `name` is the exact device identity (on Linux the PulseAudio source or sink
/// name); `label` is display-only. Equality and hashing ignore everything but
/// `name` and `device_type`.
#[derive(Clone, Serialize, Debug)]
pub struct AudioDevice {
    pub name: String,
    pub device_type: DeviceType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub is_default: bool,
}

impl PartialEq for AudioDevice {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.device_type == other.device_type
    }
}

impl Eq for AudioDevice {}

impl std::hash::Hash for AudioDevice {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.device_type.hash(state);
    }
}

impl AudioDevice {
    pub fn new(name: String, device_type: DeviceType) -> Self {
        AudioDevice {
            name,
            device_type,
            label: None,
            is_default: false,
        }
    }

    pub fn with_label(mut self, label: String) -> Self {
        self.label = Some(label);
        self
    }

    pub fn with_default(mut self, is_default: bool) -> Self {
        self.is_default = is_default;
        self
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

/// Builds the device for an exact ID whose role is known from the caller
/// (microphone or system parameter). IDs are never parsed for a type suffix:
/// a real device name may legitimately end with "(input)" or "(output)".
pub fn device_from_id(id: &str, device_type: DeviceType) -> Result<AudioDevice> {
    if id.trim().is_empty() {
        return Err(anyhow!("Device name cannot be empty"));
    }
    Ok(AudioDevice::new(id.to_string(), device_type))
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

        match audio_device.device_type {
            DeviceType::Input => {
                #[cfg(target_os = "linux")]
                let _guard = super::platform::linux::ALSA_GLOBAL_LOCK
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());

                for device in host.input_devices()? {
                    if let Ok(name) = device.name() {
                        if name == audio_device.name {
                            let default_config = device
                                .default_input_config()
                                .map_err(|e| anyhow!("Failed to get default input config: {}", e))?;
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
                    // For Linux, we use PulseAudio monitor sources for system audio.
                    // Serialize every alsa-lib call: see ALSA_GLOBAL_LOCK doc comment.
                    let _guard = super::platform::linux::ALSA_GLOBAL_LOCK
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());

                    if let Ok(pulse_host) = cpal::host_from_id(cpal::HostId::Alsa) {
                        for device in pulse_host.input_devices()? {
                            if let Ok(name) = device.name() {
                                if name == audio_device.name {
                                    let default_config = device
                                        .default_input_config()
                                        .map_err(|e| anyhow!("Failed to get default input config: {}", e))?;
                                    return Ok((device, default_config));
                                }
                            }
                        }
                    }
                }
            }
        }

        Err(anyhow!("Device not found: {}", audio_device.name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn equality_ignores_label_and_default_flag() {
        let listed = AudioDevice::new("src_a".into(), DeviceType::Input)
            .with_label("Mic".into())
            .with_default(true);
        let parsed = AudioDevice::from_name("src_a (input)").unwrap();
        assert_eq!(listed, parsed);
    }

    #[test]
    fn equality_distinguishes_name_and_type() {
        let input = AudioDevice::new("x".into(), DeviceType::Input);
        assert_ne!(input, AudioDevice::new("y".into(), DeviceType::Input));
        assert_ne!(input, AudioDevice::new("x".into(), DeviceType::Output));
    }

    #[test]
    fn hash_is_consistent_with_equality() {
        let mut set = HashSet::new();
        set.insert(AudioDevice::new("x".into(), DeviceType::Input).with_label("A".into()));
        assert!(set.contains(&AudioDevice::new("x".into(), DeviceType::Input)));
        assert!(!set.contains(&AudioDevice::new("x".into(), DeviceType::Output)));
    }

    #[test]
    fn serialization_omits_absent_label() {
        let json = serde_json::to_value(AudioDevice::new("x".into(), DeviceType::Input)).unwrap();
        assert!(json.get("label").is_none());
        assert_eq!(json["is_default"], false);
        let json = serde_json::to_value(
            AudioDevice::new("x".into(), DeviceType::Output).with_label("L".into()),
        )
        .unwrap();
        assert_eq!(json["label"], "L");
    }

    #[test]
    fn device_from_id_keeps_the_exact_id_even_with_a_type_like_suffix() {
        let plain = device_from_id("MacBook Pro Microphone", DeviceType::Input).unwrap();
        assert_eq!(plain.name, "MacBook Pro Microphone");
        assert_eq!(plain.device_type, DeviceType::Input);
        let odd = device_from_id("Weird Mic (input)", DeviceType::Output).unwrap();
        assert_eq!(odd.name, "Weird Mic (input)");
        assert_eq!(odd.device_type, DeviceType::Output);
        assert!(device_from_id("  ", DeviceType::Input).is_err());
    }
}
