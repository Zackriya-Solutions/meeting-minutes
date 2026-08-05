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

/// How many inputs in `listed` answer to `name`.
///
/// An `AudioDevice` carries a display name and nothing else, so a name two
/// inputs share cannot be resolved to either of them. Callers use this to
/// refuse to guess.
#[cfg(target_os = "macos")]
fn inputs_named(listed: &[AudioDevice], name: &str) -> usize {
    listed
        .iter()
        .filter(|device| device.device_type == DeviceType::Input && device.name == name)
        .count()
}

/// Whether exactly one input answers to `name`, according to Core Audio's
/// hardware properties.
///
/// Deliberately *not* asked of `host.input_devices()`: that probes every input
/// by opening a temporary AudioUnit, which is the microphone contention the
/// fast path below exists to avoid. Core Audio's property list answers the same
/// question without touching a stream. A failure to enumerate answers "no", so
/// an unreadable device list costs a fast path, never a wrong device.
#[cfg(target_os = "macos")]
fn input_name_is_unambiguous(host: &cpal::Host, name: &str) -> bool {
    match super::platform::configure_macos_audio(host) {
        Ok(listed) => inputs_named(&listed, name) == 1,
        Err(error) => {
            log::debug!("Could not confirm '{name}' is the only input by that name: {error}");
            false
        }
    }
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
                // The loop below calls `host.input_devices()`, which probes every
                // input on macOS by opening a temporary AudioUnit — the contention
                // this module exists to avoid. Asking for the default device by
                // name never probes anything, and the default is what the user is
                // recording with almost every time.
                //
                // Only when the name picks out exactly one input, though. Two
                // inputs can share a display name (a pair of identical USB mics,
                // a duplicated driver entry), and a name is all an `AudioDevice`
                // carries, so answering such a request with the OS default would
                // be a guess. That case gives up the shortcut and falls through to
                // the enumeration, which is what ran here before this fast path
                // existed — no worse than it was, and never silently the default.
                #[cfg(target_os = "macos")]
                if let Some(device) = host.default_input_device() {
                    if device.name().ok().as_deref() == Some(audio_device.name.as_str())
                        && input_name_is_unambiguous(&host, &audio_device.name)
                    {
                        let default_config = device
                            .default_input_config()
                            .map_err(|e| anyhow!("Failed to get default input config: {}", e))?;
                        return Ok((device, default_config));
                    }
                }

                for device in host.input_devices()? {
                    if let Ok(name) = device.name() {
                        if name == audio_device.name {
                            let default_config = device.default_input_config().map_err(|e| {
                                anyhow!("Failed to get default input config: {}", e)
                            })?;
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
                    // For Linux, we use PulseAudio monitor sources for system audio
                    if let Ok(pulse_host) = cpal::host_from_id(cpal::HostId::Alsa) {
                        for device in pulse_host.input_devices()? {
                            if let Ok(name) = device.name() {
                                if name == audio_device.name {
                                    let default_config =
                                        device.default_input_config().map_err(|e| {
                                            anyhow!("Failed to get default input config: {}", e)
                                        })?;
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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    fn input(name: &str) -> AudioDevice {
        AudioDevice::new(name.to_string(), DeviceType::Input)
    }

    /// The macOS fast path answers with the OS default device, so it may only
    /// run when the requested name belongs to exactly one input. Two inputs
    /// sharing a name must send the caller down the enumeration instead.
    #[test]
    fn a_name_two_inputs_share_is_not_unambiguous() {
        let listed = vec![
            input("Shure MV7"),
            input("Shure MV7"),
            input("MacBook Pro Microphone"),
            AudioDevice::new("Shure MV7".to_string(), DeviceType::Output),
        ];

        assert_eq!(
            inputs_named(&listed, "Shure MV7"),
            2,
            "the fast path must not fire"
        );
        assert_eq!(inputs_named(&listed, "MacBook Pro Microphone"), 1);
        // An output of the same name is a different device and must not count
        // towards the ambiguity of an input.
        assert_eq!(inputs_named(&listed, "Nothing By This Name"), 0);
    }
}
