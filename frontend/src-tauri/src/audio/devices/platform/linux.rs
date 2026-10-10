use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};

use crate::audio::devices::configuration::{AudioDevice, DeviceType};

/// True when an ALSA capture device name denotes a sink monitor, i.e. it
/// records what the system is playing rather than a microphone. The single
/// definition of "system audio device" on Linux: both the device list and the
/// default system-audio choice (speakers::default_output_device) use it.
pub fn is_monitor_device_name(name: &str) -> bool {
    name.to_lowercase().contains("monitor")
}

/// The ALSA PCM name the Linux setup docs tell users to define
/// (`pcm.system_monitor { type pulse; device "@DEFAULT_MONITOR@" }`).
pub const PREFERRED_MONITOR_PCM: &str = "system_monitor";

/// Suffix older builds appended to monitor names in the device list. A
/// preference saved from one of those builds still carries it, so strip it
/// before the exact-name lookup or that user silently loses system audio.
const LEGACY_SYSTEM_AUDIO_SUFFIX: &str = " (System Audio)";

pub fn strip_legacy_suffix(name: &str) -> &str {
    name.strip_suffix(LEGACY_SYSTEM_AUDIO_SUFFIX).unwrap_or(name)
}

/// Monitor device to use by default: the documented `system_monitor` PCM if
/// present, otherwise the first monitor in enumeration order. Preferring the
/// exact name keeps the choice stable when some other device (a mic sold as
/// a "Studio Monitor") also contains the word.
pub fn first_monitor_device_name<I: IntoIterator<Item = String>>(names: I) -> Option<String> {
    let monitors: Vec<String> = names.into_iter().filter(|n| is_monitor_device_name(n)).collect();
    monitors
        .iter()
        .find(|n| n.as_str() == PREFERRED_MONITOR_PCM)
        .or_else(|| monitors.first())
        .cloned()
}

/// Configure Linux audio devices using ALSA/PulseAudio
pub fn configure_linux_audio(host: &cpal::Host) -> Result<Vec<AudioDevice>> {
    let mut devices = Vec::new();

    // Add input devices
    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            devices.push(AudioDevice::new(name, DeviceType::Input));
        }
    }

    // Add PulseAudio monitor sources for system audio
    if let Ok(pulse_host) = cpal::host_from_id(cpal::HostId::Alsa) {
        for device in pulse_host.input_devices()? {
            if let Ok(name) = device.name() {
                // Check if it's a monitor source
                if is_monitor_device_name(&name) {
                    // Keep the raw ALSA name: get_device_and_config looks the
                    // device up by exact name, so a display suffix here made
                    // every selected monitor fail with "Device not found".
                    devices.push(AudioDevice::new(name, DeviceType::Output));
                }
            }
        }
    }

    Ok(devices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_alsa_device_is_not_a_monitor() {
        // "default" is the mic as an ALSA input; picking it duplicated the mic.
        assert!(!is_monitor_device_name("default"));
        assert!(!is_monitor_device_name("pipewire"));
    }

    #[test]
    fn monitor_names_match_case_insensitively() {
        assert!(is_monitor_device_name("system_monitor"));
        assert!(is_monitor_device_name("Speakers.Monitor"));
    }

    #[test]
    fn first_monitor_skips_default_and_mics() {
        let names = ["default", "pipewire", "hw:Elgato", "system_monitor", "other_monitor"]
            .map(String::from);
        assert_eq!(first_monitor_device_name(names), Some("system_monitor".to_string()));
    }

    #[test]
    fn documented_pcm_wins_over_earlier_monitor_named_device() {
        let names = ["default", "Studio Monitor Mic", "system_monitor"].map(String::from);
        assert_eq!(first_monitor_device_name(names), Some("system_monitor".to_string()));
    }

    #[test]
    fn legacy_suffix_is_stripped_and_plain_names_untouched() {
        assert_eq!(strip_legacy_suffix("system_monitor (System Audio)"), "system_monitor");
        assert_eq!(strip_legacy_suffix("system_monitor"), "system_monitor");
    }

    #[test]
    fn no_monitor_yields_none() {
        let names = ["default", "pipewire"].map(String::from);
        assert_eq!(first_monitor_device_name(names), None);
    }
}
