use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};

use crate::audio::devices::configuration::{AudioDevice, DeviceType};

/// Returns `false` for ALSA PCM device names that are raw permutations
/// users don't care about (front/surround/iec958/hw/dmix/dsnoop/etc.).
/// Logical endpoints (`default`, `pipewire`, `pulse`, `sysdefault`) and
/// friendly device names pass through unchanged.
///
/// Issue: #437 — Linux device pickers showed every ALSA profile per card.
pub fn is_meaningful_alsa_pcm(name: &str) -> bool {
    const BANNED_PREFIXES: &[&str] = &[
        "front:",
        "rear:",
        "center_lfe:",
        "side:",
        "surround",      // surround21/40/41/50/51/71:CARD=...
        "iec958:",
        "spdif:",
        "hw:",
        "plughw:",
        "dmix:",
        "dsnoop:",
        "usbstream:",
        "sysdefault:",   // per-card sysdefault (top-level "sysdefault" passes)
    ];
    !BANNED_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// Configure Linux audio devices using ALSA/PulseAudio
pub fn configure_linux_audio(host: &cpal::Host) -> Result<Vec<AudioDevice>> {
    let mut devices = Vec::new();

    // Add input devices, skipping raw ALSA PCM permutations (#437).
    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            if is_meaningful_alsa_pcm(&name) {
                devices.push(AudioDevice::new(name, DeviceType::Input));
            }
        }
    }

    // Add PulseAudio monitor sources for system audio
    if let Ok(pulse_host) = cpal::host_from_id(cpal::HostId::Alsa) {
        for device in pulse_host.input_devices()? {
            if let Ok(name) = device.name() {
                // Monitor sources contain "monitor" in their name and don't
                // start with any of the banned PCM prefixes, so they pass
                // is_meaningful_alsa_pcm naturally — but we still gate on the
                // "monitor" substring to label them as system-audio outputs.
                if name.contains("monitor") {
                    devices.push(AudioDevice::new(
                        format!("{} (System Audio)", name),
                        DeviceType::Output,
                    ));
                }
            }
        }
    }

    Ok(devices)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- KEEP cases --

    #[test]
    fn keeps_default_endpoint() {
        assert!(is_meaningful_alsa_pcm("default"));
    }

    #[test]
    fn keeps_pipewire_endpoint() {
        assert!(is_meaningful_alsa_pcm("pipewire"));
    }

    #[test]
    fn keeps_pulse_endpoint() {
        assert!(is_meaningful_alsa_pcm("pulse"));
    }

    #[test]
    fn keeps_toplevel_sysdefault() {
        assert!(is_meaningful_alsa_pcm("sysdefault"));
    }

    #[test]
    fn keeps_friendly_usb_mic_name() {
        assert!(is_meaningful_alsa_pcm("Yeti X"));
        assert!(is_meaningful_alsa_pcm("AT2020USB+ Mono"));
    }

    #[test]
    fn keeps_pipewire_node_name() {
        assert!(is_meaningful_alsa_pcm(
            "alsa_input.usb-Blue_Microphones_Yeti_X-00.analog-stereo"
        ));
        assert!(is_meaningful_alsa_pcm(
            "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor"
        ));
    }

    #[test]
    fn keeps_builtin_audio_name() {
        assert!(is_meaningful_alsa_pcm("HDA Intel PCH"));
        assert!(is_meaningful_alsa_pcm("Built-in Audio Analog Stereo"));
    }

    #[test]
    fn keeps_empty_string() {
        assert!(is_meaningful_alsa_pcm(""));
    }

    // -- FILTER cases (every example from the issue) --

    #[test]
    fn filters_per_card_sysdefault() {
        assert!(!is_meaningful_alsa_pcm("sysdefault:CARD=NTUSB"));
        assert!(!is_meaningful_alsa_pcm("sysdefault:CARD=Generic"));
    }

    #[test]
    fn filters_front_stereo_pcm() {
        assert!(!is_meaningful_alsa_pcm("front:CARD=NTUSB,DEV=0"));
        assert!(!is_meaningful_alsa_pcm("front:CARD=Generic,DEV=0"));
    }

    #[test]
    fn filters_surround_variants() {
        for variant in [
            "surround21", "surround40", "surround41", "surround50", "surround51", "surround71",
        ] {
            assert!(
                !is_meaningful_alsa_pcm(&format!("{variant}:CARD=Generic,DEV=0")),
                "expected {variant} variant to be filtered"
            );
        }
    }

    #[test]
    fn filters_iec958_and_spdif() {
        assert!(!is_meaningful_alsa_pcm("iec958:CARD=NTUSB,DEV=0"));
        assert!(!is_meaningful_alsa_pcm("spdif:CARD=Generic,DEV=1"));
    }

    #[test]
    fn filters_raw_hardware_handles() {
        assert!(!is_meaningful_alsa_pcm("hw:0,0"));
        assert!(!is_meaningful_alsa_pcm("hw:CARD=NTUSB,DEV=0"));
        assert!(!is_meaningful_alsa_pcm("plughw:0,0"));
    }

    #[test]
    fn filters_software_mixing_nodes() {
        assert!(!is_meaningful_alsa_pcm("dmix:CARD=NTUSB,DEV=0"));
        assert!(!is_meaningful_alsa_pcm("dsnoop:CARD=NTUSB,DEV=0"));
    }

    #[test]
    fn filters_rear_center_side_pcms() {
        assert!(!is_meaningful_alsa_pcm("rear:CARD=Generic,DEV=0"));
        assert!(!is_meaningful_alsa_pcm("center_lfe:CARD=Generic,DEV=0"));
        assert!(!is_meaningful_alsa_pcm("side:CARD=Generic,DEV=0"));
    }

    #[test]
    fn does_not_falsely_filter_lookalike_names() {
        // Filter uses case-sensitive startsWith, so capital-S "Surrounded"
        // doesn't match "surround", and "iec958_helper" doesn't match "iec958:".
        assert!(is_meaningful_alsa_pcm("Surrounded by USB Mics"));
        assert!(is_meaningful_alsa_pcm("iec958_helper"));
    }
}
