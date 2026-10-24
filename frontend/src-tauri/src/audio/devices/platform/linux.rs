use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};
use log::{info, warn};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

use crate::audio::capture::{
    default_pulse_sink_name, default_pulse_source_name, list_pulse_sinks, list_pulse_sources,
    PulseSink, PulseSource,
};
use crate::audio::devices::configuration::{AudioDevice, DeviceType};

/// Global lock serializing every interaction with alsa-lib on Linux.
///
/// alsa-lib's config/cache functions (`snd_device_name_hint`,
/// `snd_config_update_r`) and PCM open functions (`snd_pcm_open`, called
/// internally by `default_input_config`/`default_output_config`/
/// `build_input_stream`) all mutate a process-global config cache and are not
/// safe to call concurrently from multiple threads. `list_audio_devices()` is
/// polled every few seconds by the device disconnect/reconnect monitor
/// (device_monitor.rs) while a recording is active, and PCM setup happens on
/// another Tokio worker thread whenever a cpal fallback stream starts
/// (degraded mode, stale saved preference, permission trigger, etc.). Without
/// serialization, an overlapping call corrupts alsa-lib's heap state, which
/// glibc eventually detects and aborts the process on — this is what produced
/// the SIGABRT crashes after ~30 minutes of recording.
pub(crate) static ALSA_GLOBAL_LOCK: Mutex<()> = Mutex::new(());

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

/// Last seen display label per device ID, so events about a device that has
/// just disappeared can still show its readable name.
static LABEL_CACHE: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn remember_labels(devices: &[AudioDevice]) {
    let mut cache = LABEL_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let map = cache.get_or_insert_with(HashMap::new);
    for device in devices {
        if let Some(label) = &device.label {
            map.insert(device.name.clone(), label.clone());
        }
    }
}

/// Display label for a device ID; the ID itself when no label was seen yet.
pub fn cached_label(id: &str) -> String {
    let cache = LABEL_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache
        .as_ref()
        .and_then(|map| map.get(id).cloned())
        .unwrap_or_else(|| id.to_string())
}

const GENERIC_INPUT_LABEL: &str = "Microphone";
const GENERIC_OUTPUT_LABEL: &str = "Sortie audio";

/// Display labels for one role (inputs or outputs), aligned with `entries`
/// (`(id, description)`). Empty descriptions become the generic role label,
/// never the raw ID. Devices sharing a label get a number, assigned by the
/// rank of their exact ID so it does not depend on enumeration order. A
/// generated caption never equals another device's literal description. The
/// numbering is display-only and never part of an identity.
fn role_labels(entries: &[(&str, &str)], generic: &str) -> Vec<String> {
    let base: Vec<String> = entries
        .iter()
        .map(|(_, description)| {
            let trimmed = description.trim();
            if trimmed.is_empty() {
                generic.to_string()
            } else {
                trimmed.to_string()
            }
        })
        .collect();

    let mut groups: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, label) in base.iter().enumerate() {
        groups.entry(label.as_str()).or_default().push(index);
    }

    let mut taken: HashSet<String> = base.iter().cloned().collect();
    let mut labels = base.clone();
    for (label, mut members) in groups {
        if members.len() < 2 {
            continue;
        }
        members.sort_by(|a, b| entries[*a].0.cmp(entries[*b].0).then(a.cmp(b)));
        let mut number = 1usize;
        for index in members {
            let candidate = loop {
                let candidate = format!("{} {}", label, number);
                number += 1;
                if !taken.contains(&candidate) {
                    break candidate;
                }
            };
            taken.insert(candidate.clone());
            labels[index] = candidate;
        }
    }
    labels
}

fn pulse_source_devices(sources: &[PulseSource], default_name: Option<&str>) -> Vec<AudioDevice> {
    let entries: Vec<(&str, &str)> = sources
        .iter()
        .map(|s| (s.source_name.as_str(), s.description.as_str()))
        .collect();
    let labels = role_labels(&entries, GENERIC_INPUT_LABEL);
    sources
        .iter()
        .zip(labels)
        .map(|(source, label)| {
            AudioDevice::new(source.source_name.clone(), DeviceType::Input)
                .with_label(label)
                .with_default(Some(source.source_name.as_str()) == default_name)
        })
        .collect()
}

fn pulse_sink_devices(sinks: &[PulseSink], default_name: Option<&str>) -> Vec<AudioDevice> {
    let entries: Vec<(&str, &str)> = sinks
        .iter()
        .map(|s| (s.name.as_str(), s.description.as_str()))
        .collect();
    let labels = role_labels(&entries, GENERIC_OUTPUT_LABEL);
    sinks
        .iter()
        .zip(labels)
        .map(|(sink, label)| {
            AudioDevice::new(sink.name.clone(), DeviceType::Output)
                .with_label(label)
                .with_default(Some(sink.name.as_str()) == default_name)
        })
        .collect()
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
            let default_source = default_pulse_source_name().unwrap_or_else(|e| {
                warn!("configure_linux_audio: default source query failed: {}", e);
                None
            });
            devices.extend(pulse_source_devices(&sources, default_source.as_deref()));
        }
        Err(e) => {
            warn!(
                "🎙️ configure_linux_audio: failed to list PulseAudio/PipeWire sources ({}); falling back to filtered ALSA endpoints",
                e
            );

            // Serialize every alsa-lib call: see ALSA_GLOBAL_LOCK doc comment above.
            let input_devices: Vec<_> = {
                let _guard = ALSA_GLOBAL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
            let default_sink = default_pulse_sink_name().unwrap_or_else(|e| {
                warn!("configure_linux_audio: default sink query failed: {}", e);
                None
            });
            devices.extend(pulse_sink_devices(&sinks, default_sink.as_deref()));
        }
        Err(e) => {
            warn!("Failed to list PulseAudio/PipeWire sinks for System Audio: {}", e);
        }
    }

    remember_labels(&devices);
    Ok(devices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(name: &str, description: &str) -> PulseSource {
        PulseSource {
            description: description.to_string(),
            source_name: name.to_string(),
        }
    }

    fn sink(name: &str, description: &str) -> PulseSink {
        PulseSink {
            name: name.to_string(),
            description: description.to_string(),
            monitor_source_name: format!("{}.monitor", name),
        }
    }

    #[test]
    fn cached_label_falls_back_to_id_then_serves_remembered_label() {
        assert_eq!(cached_label("cache_test_id"), "cache_test_id");
        remember_labels(&[AudioDevice::new("cache_test_id".into(), DeviceType::Input)
            .with_label("Readable".into())]);
        assert_eq!(cached_label("cache_test_id"), "Readable");
    }

    #[test]
    fn unique_labels_are_left_alone() {
        let devices = pulse_source_devices(
            &[source("alsa_input.a", "Mic A"), source("alsa_input.b", "Mic B")],
            None,
        );
        assert_eq!(devices[0].label.as_deref(), Some("Mic A"));
        assert_eq!(devices[1].label.as_deref(), Some("Mic B"));
    }

    fn labels_of(devices: &[AudioDevice]) -> Vec<String> {
        devices.iter().map(|d| d.label.clone().unwrap()).collect()
    }

    fn assert_distinct(labels: &[String]) {
        let unique: HashSet<&String> = labels.iter().collect();
        assert_eq!(unique.len(), labels.len(), "labels collide: {labels:?}");
    }

    #[test]
    fn duplicate_descriptions_get_distinct_labels_without_ids_but_keep_exact_ids() {
        let devices = pulse_source_devices(
            &[
                source("alsa_input.usb-1.analog", "USB Mic"),
                source("alsa_input.usb-2.analog", "USB Mic"),
            ],
            None,
        );
        assert_eq!(devices[0].name, "alsa_input.usb-1.analog");
        assert_eq!(devices[1].name, "alsa_input.usb-2.analog");
        let labels = labels_of(&devices);
        assert_distinct(&labels);
        for label in &labels {
            assert!(label.starts_with("USB Mic"));
            assert!(!label.contains("usb-1") && !label.contains("usb-2"));
            assert!(!label.contains("alsa_input"));
        }
    }

    #[test]
    fn ids_sharing_a_short_form_are_still_distinguished() {
        let devices = pulse_sink_devices(
            &[sink("a.same", "Same device"), sink("b.same", "Same device")],
            None,
        );
        let labels = labels_of(&devices);
        assert_distinct(&labels);
        assert!(labels.iter().all(|l| !l.contains("same)") && !l.contains("a.same")));
    }

    #[test]
    fn generated_label_never_collides_with_another_literal_description() {
        let probe = pulse_source_devices(
            &[source("id.1", "Mic"), source("id.2", "Mic")],
            None,
        );
        let generated = labels_of(&probe);
        for literal in &generated {
            let devices = pulse_source_devices(
                &[
                    source("id.1", "Mic"),
                    source("id.2", "Mic"),
                    source("id.3", literal),
                ],
                None,
            );
            let labels = labels_of(&devices);
            assert_distinct(&labels);
            assert_eq!(devices[2].label.as_deref(), Some(literal.as_str()));
        }
    }

    #[test]
    fn labels_do_not_depend_on_enumeration_order() {
        let forward = [
            source("id.c", "Mic"),
            source("id.a", "Mic"),
            source("id.b", "Mic"),
            source("id.z", "Other"),
        ];
        let mut reversed = forward.to_vec();
        reversed.reverse();
        let by_id = |devices: Vec<AudioDevice>| -> BTreeMap<String, String> {
            devices
                .into_iter()
                .map(|d| (d.name.clone(), d.label.clone().unwrap()))
                .collect()
        };
        let a = by_id(pulse_source_devices(&forward, None));
        let b = by_id(pulse_source_devices(&reversed, None));
        assert_eq!(a, b);
        assert_distinct(&a.values().cloned().collect::<Vec<_>>());
    }

    #[test]
    fn empty_descriptions_of_several_devices_are_distinct_too() {
        let devices = pulse_sink_devices(&[sink("s.1", ""), sink("s.2", "  ")], None);
        assert_distinct(&labels_of(&devices));
    }

    #[test]
    fn ids_and_default_flags_are_untouched_by_labelling() {
        let devices = pulse_sink_devices(
            &[sink("s.b", "Same"), sink("s.a", "Same")],
            Some("s.a"),
        );
        assert_eq!(devices[0].name, "s.b");
        assert_eq!(devices[1].name, "s.a");
        assert!(!devices[0].is_default && devices[1].is_default);
    }

    #[test]
    fn same_description_across_roles_is_not_disambiguated() {
        let inputs = pulse_source_devices(&[source("alsa_input.h", "Headset")], None);
        let outputs = pulse_sink_devices(&[sink("alsa_output.h", "Headset")], None);
        assert_eq!(inputs[0].label.as_deref(), Some("Headset"));
        assert_eq!(outputs[0].label.as_deref(), Some("Headset"));
        assert_ne!(inputs[0], outputs[0]);
    }

    #[test]
    fn empty_description_gets_generic_role_label_not_raw_id() {
        let inputs = pulse_source_devices(&[source("alsa_input.x", "  ")], None);
        let outputs = pulse_sink_devices(&[sink("alsa_output.x", "")], None);
        assert_eq!(inputs[0].label.as_deref(), Some("Microphone"));
        assert_eq!(outputs[0].label.as_deref(), Some("Sortie audio"));
    }

    #[test]
    fn default_flag_marks_exact_id_only() {
        let devices = pulse_sink_devices(
            &[sink("sink_a", "Speakers"), sink("sink_b", "Speakers")],
            Some("sink_b"),
        );
        assert!(!devices[0].is_default);
        assert!(devices[1].is_default);
    }

    #[test]
    fn sink_device_name_has_no_display_suffix() {
        let devices = pulse_sink_devices(&[sink("sink_a", "Speakers")], None);
        assert_eq!(devices[0].name, "sink_a");
        assert_eq!(devices[0].device_type, DeviceType::Output);
    }

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