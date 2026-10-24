use anyhow::{anyhow, Result};
use cpal::traits::{HostTrait, DeviceTrait};
use log::{info, warn};

use super::configuration::{AudioDevice, DeviceType};

/// Get the default input (microphone) device for the system.
///
/// On Linux, ask PulseAudio/PipeWire for the exact default source name so that
/// "Default Microphone" points to the same item shown in the device list. Fall
/// back to cpal's default input device if the server is unreachable.
pub fn default_input_device() -> Result<AudioDevice> {
    #[cfg(target_os = "linux")]
    {
        match crate::audio::capture::default_pulse_source_name() {
            Ok(Some(source_name)) => {
                match crate::audio::capture::list_pulse_sources() {
                    Ok(sources) => {
                        return eligible_default_source(&source_name, &sources)
                            .map(|id| AudioDevice::new(id, DeviceType::Input).with_default(true));
                    }
                    Err(e) => warn!(
                        "🎤 Failed to list PulseAudio sources ({}), falling back to CPAL",
                        e
                    ),
                }
            }
            Ok(None) => {
                warn!("🎤 No default PulseAudio source reported, falling back to CPAL");
            }
            Err(e) => {
                warn!(
                    "🎤 Failed to query default PulseAudio source ({}), falling back to CPAL",
                    e
                );
            }
        }
    }

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("No default input device found"))?;
    Ok(AudioDevice::new(device.name()?, DeviceType::Input))
}

/// The server's default source is only a usable microphone if capture can
/// resolve it, i.e. it is in the microphone source list (sink monitors are
/// excluded there). An excluded default is an error, never another microphone
/// and never an ALSA name.
#[cfg(target_os = "linux")]
fn eligible_default_source(
    default_name: &str,
    sources: &[crate::audio::capture::PulseSource],
) -> Result<String> {
    if sources.iter().any(|s| s.source_name == default_name) {
        Ok(default_name.to_string())
    } else {
        Err(anyhow!(
            "Default input source '{}' is not a usable microphone (e.g. a playback monitor)",
            default_name
        ))
    }
}

/// Find the built-in microphone device (wired, stable, consistent sample rate)
///
/// Searches for MacBook/built-in microphone patterns to find the hardware
/// microphone instead of Bluetooth devices. This is useful for:
/// - Avoiding Bluetooth variable sample rate issues
/// - Getting stable wired audio for recording
/// - Fallback when Bluetooth device is default but unreliable
///
/// Returns None if no built-in microphone found
pub fn find_builtin_input_device() -> Result<Option<AudioDevice>> {
    let host = cpal::default_host();

    // Built-in microphone name patterns (platform-specific)
    let builtin_patterns = [
        // macOS patterns
        "macbook",
        "built-in microphone",
        "internal microphone",
        // Windows patterns
        "microphone array",
        "realtek",
        "conexant",
        // Linux patterns
        "hda intel",
        "built-in audio",
    ];

    // Search all input devices for built-in pattern matches
    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            let name_lower = name.to_lowercase();

            // Check if this is a built-in device
            for pattern in &builtin_patterns {
                if name_lower.contains(pattern) {
                    // Additional filter: exclude Bluetooth/wireless devices
                    if name_lower.contains("bluetooth") ||
                       name_lower.contains("airpods") ||
                       name_lower.contains("wireless") {
                        continue; // Skip Bluetooth devices
                    }

                    info!("🎤 Found built-in microphone: '{}'", name);
                    return Ok(Some(AudioDevice::new(name, DeviceType::Input)));
                }
            }
        }
    }

    warn!("⚠️ No built-in microphone found (searched {} patterns)", builtin_patterns.len());
    Ok(None)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::audio::capture::PulseSource;

    fn source(name: &str) -> PulseSource {
        PulseSource {
            description: name.to_string(),
            source_name: name.to_string(),
        }
    }

    #[test]
    fn eligible_non_first_default_resolves_to_that_exact_source() {
        let sources = [source("alsa_input.a"), source("alsa_input.b")];
        assert_eq!(
            eligible_default_source("alsa_input.b", &sources).unwrap(),
            "alsa_input.b"
        );
    }

    #[test]
    fn sink_monitor_default_is_rejected_not_replaced() {
        let sources = [source("alsa_input.a")];
        assert!(eligible_default_source("alsa_output.speakers.monitor", &sources).is_err());
    }

    #[test]
    fn empty_source_list_rejects_any_default() {
        assert!(eligible_default_source("alsa_input.a", &[]).is_err());
    }
}
