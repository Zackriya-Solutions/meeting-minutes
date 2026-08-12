use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};
use log::{info, warn};
use std::sync::Mutex;

use crate::audio::capture::{list_pulse_sinks, list_pulse_sources};
use crate::audio::devices::configuration::{AudioDevice, DeviceType};

/// alsa-lib's device/config enumeration (`snd_device_name_hint`,
/// `snd_config_update_r`) is not safe to call concurrently from multiple
/// threads: it mutates a process-global config cache. `list_audio_devices()`
/// is polled every few seconds by the device disconnect/reconnect monitor
/// (device_monitor.rs) while a recording is active, and can also be invoked
/// at any time from the UI (device list refresh) or when starting a new
/// recording (recording_manager.rs). Without serialization, an overlapping
/// call from a second Tokio worker thread corrupts alsa-lib's heap state,
/// which glibc eventually detects and aborts the process on — this is what
/// produced the SIGABRT crashes after ~30 minutes of recording.
pub(crate) static ALSA_ENUM_LOCK: Mutex<()> = Mutex::new(());

/// Degraded-mode filter: which raw cpal/ALSA PCM names are worth showing
/// when the PulseAudio/PipeWire server can't be reached.
///
/// A whitelist is deliberately used instead of a blacklist: ALSA exposes many
/// plugins, rate converters, and user-defined aliases (~/.asoundrc) that all
/// look like plausible devices. Blacklisting them is endless and still leaves
/// the list confusing. The whitelist gives a bounded, predictable fallback.
pub(crate) fn is_useful_alsa_endpoint(name: &str) -> bool {
    matches!(name.to_lowercase().as_str(), "default" | "pipewire" | "pulse")
}

/// Configure Linux audio devices.
///
/// In the nominal case (PulseAudio/PipeWire server reachable) both the
/// microphone list and the system-audio list come from the server's own
/// introspection: real human-readable descriptions, no ~/.asoundrc monitor-hint
/// scanning, and no cpal/ALSA enumeration at all. This is the only way to show
/// the same labels as the desktop's native audio settings (KDE, GNOME…).
///
/// If the server is unreachable, fall back to a short whitelist of ALSA
/// endpoints (`default`, `pipewire`, `pulse`) so the list is never empty.
///
/// See audio/capture/pulse_linux.rs for the actual capture implementation.
pub fn configure_linux_audio(host: &cpal::Host) -> Result<Vec<AudioDevice>> {
    let mut devices = Vec::new();

    // ------------------------------------------------------------------
    // Microphones — nominal path: real PulseAudio/PipeWire input sources.
    // ------------------------------------------------------------------
    match list_pulse_sources() {
        Ok(sources) => {
            info!(
                "🎙️ configure_linux_audio: PulseAudio/PipeWire gave {} input source(s)",
                sources.len()
            );
            for source in sources {
                devices.push(AudioDevice::new(source.description, DeviceType::Input));
            }
        }
        Err(e) => {
            warn!(
                "🎙️ configure_linux_audio: failed to list PulseAudio/PipeWire sources ({}); falling back to filtered ALSA endpoints",
                e
            );

            // Serialize ALSA enumeration: see ALSA_ENUM_LOCK doc comment above.
            let input_devices: Vec<_> = {
                let _guard = ALSA_ENUM_LOCK.lock().unwrap_or_else(|e| e.into_inner());
                host.input_devices()?.collect()
            };

            let mut kept_any = false;
            for device in input_devices {
                if let Ok(name) = device.name() {
                    if is_useful_alsa_endpoint(&name) {
                        devices.push(AudioDevice::new(name, DeviceType::Input));
                        kept_any = true;
                    }
                }
            }

            if !kept_any {
                warn!("🎙️ configure_linux_audio: no useful ALSA endpoint found; adding 'default' so the list is never empty");
                devices.push(AudioDevice::new("default".to_string(), DeviceType::Input));
            }
        }
    }

    // ------------------------------------------------------------------
    // System Audio — unchanged from US-01: PulseAudio/PipeWire sinks.
    // ------------------------------------------------------------------
    match list_pulse_sinks() {
        Ok(sinks) => {
            info!(
                "🎙️ configure_linux_audio: list_pulse_sinks returned {} sink(s)",
                sinks.len()
            );
            for sink in sinks {
                devices.push(AudioDevice::new(
                    format!("{} (System Audio)", sink.description),
                    DeviceType::Output,
                ));
            }
        }
        Err(e) => {
            warn!("Failed to list PulseAudio/PipeWire sinks for System Audio: {}", e);
        }
    }

    Ok(devices)
}

#[cfg(test)]
mod tests {
    use super::is_useful_alsa_endpoint;

    #[test]
    fn test_is_useful_alsa_endpoint_keeps_whitelist() {
        for name in ["default", "pipewire", "pulse", "DEFAULT", "PipeWire", "PULSE"] {
            assert!(
                is_useful_alsa_endpoint(name),
                "'{}' should be kept as a useful ALSA endpoint",
                name
            );
        }
    }

    #[test]
    fn test_is_useful_alsa_endpoint_rejects_noise() {
        let rejected = [
            "null",
            "lavrate",
            "samplerate",
            "speexrate",
            "jack",
            "oss",
            "speex",
            "upmix",
            "vdownmix",
            "usbstream:CARD=Generic",
            "sysdefault:CARD=Generic_1",
            "sysdefault:CARD=acppdmmach",
            "sysdefault:CARD=J380",
            "hw:0,0",
            "plughw:0,0",
            "front:CARD=Generic,DEV=0",
            "surround21:CARD=Generic",
            "iec958:CARD=Generic",
            "hdmi:CARD=HDMI,DEV=0",
            "dmix:CARD=Generic",
            "dsnoop:CARD=Generic",
            "Ryzen_HD_Audio_Controller_Speaker_monitor",
        ];

        for name in rejected {
            assert!(
                !is_useful_alsa_endpoint(name),
                "'{}' should be rejected as ALSA noise",
                name
            );
        }
    }
}