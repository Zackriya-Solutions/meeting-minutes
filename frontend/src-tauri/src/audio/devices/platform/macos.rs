use anyhow::{anyhow, Result};
use cidre::at::AudioBufListN;
use cidre::core_audio::hardware::System;
use cidre::os;
use log::debug;

use crate::audio::devices::configuration::{AudioDevice, DeviceType};

/// Whether a device exposes capture or playback channels in one scope.
///
/// Most devices answer for both scopes and report zero channels for the one
/// they do not serve, so an error here is unusual: a device in the middle of a
/// transition, or a virtual device that publishes its configuration late. Such
/// a device drops out of the list entirely, so log why rather than leaving the
/// user with a microphone that silently disappeared. `list_audio_devices` runs
/// every couple of seconds from the disconnect monitor, hence debug level.
fn scope_has_channels(cfg: os::Result<AudioBufListN>, device_name: &str, scope: &str) -> bool {
    match cfg {
        Ok(cfg) => cfg
            .buffers()
            .iter()
            .any(|buffer| buffer.number_channels > 0),
        Err(error) => {
            debug!(
                "Core Audio device '{device_name}' reported no {scope} stream configuration: {error}"
            );
            false
        }
    }
}

/// Filter function to exclude macOS built-in speakers for output devices
///
/// NOTE: AirPods and other Bluetooth devices are now allowed (with device
/// monitoring for disconnect handling). Only built-in speakers are filtered out
/// — they don't typically capture system audio properly.
fn should_include_output_device(name: &str) -> bool {
    !name.to_lowercase().contains("speakers")
}

/// Configure macOS audio devices using ScreenCaptureKit and CoreAudio
///
/// CPAL's macOS `input_devices()` implementation probes every device by creating
/// a temporary AudioUnit. Doing that while a recording stream is starting (or
/// every two seconds from the disconnect monitor) can contend with the real
/// microphone AudioUnit and make Core Audio reconfigure the active input.
/// Core Audio's hardware property API gives us the same device list without
/// opening any capture streams.
pub fn configure_macos_audio(_host: &cpal::Host) -> Result<Vec<AudioDevice>> {
    let mut devices: Vec<AudioDevice> = Vec::new();

    let core_audio_devices = System::devices()
        .map_err(|error| anyhow!("Failed to enumerate Core Audio devices: {error}"))?;

    for device in core_audio_devices {
        let name = match device.name() {
            Ok(name) => name.to_string(),
            Err(error) => {
                debug!("Skipping Core Audio device {device:?} with no readable name: {error}");
                continue;
            }
        };

        if scope_has_channels(device.input_stream_cfg(), &name, "input") {
            devices.push(AudioDevice::new(name.clone(), DeviceType::Input));
        }

        if scope_has_channels(device.output_stream_cfg(), &name, "output")
            && should_include_output_device(&name)
        {
            devices.push(AudioDevice::new(name, DeviceType::Output));
        }
    }

    Ok(devices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait};

    /// Core Audio's hardware properties must surface at least the devices the
    /// CPAL path surfaced, or swapping one for the other quietly takes devices
    /// away from the user's picker.
    ///
    /// The old macOS path was CPAL's `input_devices()` / `output_devices()`
    /// plus a sweep of `host.devices()` in `discovery.rs` that appended
    /// anything left over as an output, so this reconstructs all three.
    ///
    /// Ignored by default: it depends on whatever hardware is attached, and it
    /// runs the CPAL probing this module exists to avoid — harmless with no
    /// recording in flight, but not something to do on every `cargo test`.
    ///
    ///     cargo test --lib macos_enumeration -- --ignored --nocapture
    #[test]
    #[ignore = "reads the host's real audio hardware"]
    fn macos_enumeration_covers_every_device_the_cpal_path_reported() {
        let host = cpal::default_host();
        let core_audio = configure_macos_audio(&host).expect("Core Audio enumeration");

        let names_of = |wanted: DeviceType| -> Vec<&str> {
            core_audio
                .iter()
                .filter(|device| device.device_type == wanted)
                .map(|device| device.name.as_str())
                .collect()
        };
        let core_audio_inputs = names_of(DeviceType::Input);
        let core_audio_outputs = names_of(DeviceType::Output);

        fn cpal_names(devices: impl Iterator<Item = cpal::Device>) -> Vec<String> {
            devices.filter_map(|device| device.name().ok()).collect()
        }
        let cpal_inputs = cpal_names(host.input_devices().expect("cpal inputs"));
        let cpal_outputs: Vec<String> = cpal_names(host.output_devices().expect("cpal outputs"))
            .into_iter()
            .filter(|name| should_include_output_device(name))
            .collect();
        // The `discovery.rs` sweep that used to append the leftovers as outputs.
        let cpal_leftovers: Vec<String> = host
            .devices()
            .expect("cpal devices")
            .filter_map(|device| device.name().ok())
            .filter(|name| !cpal_inputs.contains(name) && !cpal_outputs.contains(name))
            .collect();

        println!("cpal inputs:        {cpal_inputs:?}");
        println!("core audio inputs:  {core_audio_inputs:?}");
        println!("cpal outputs:       {cpal_outputs:?}");
        println!("core audio outputs: {core_audio_outputs:?}");
        println!("cpal leftovers:     {cpal_leftovers:?}");

        for name in &cpal_inputs {
            assert!(
                core_audio_inputs.contains(&name.as_str()),
                "input '{name}' is reachable through cpal but missing from the Core Audio list"
            );
        }
        for name in &cpal_outputs {
            assert!(
                core_audio_outputs.contains(&name.as_str()),
                "output '{name}' is reachable through cpal but missing from the Core Audio list"
            );
        }
        // The leftovers were only ever labelled outputs because nothing else
        // had claimed them, so accept either classification here.
        for name in &cpal_leftovers {
            assert!(
                core_audio_inputs.contains(&name.as_str())
                    || core_audio_outputs.contains(&name.as_str())
                    || !should_include_output_device(name),
                "'{name}' was listed by the old host.devices() sweep but the Core Audio list drops it"
            );
        }
    }

    /// Every input the picker offers must still resolve to a device of that
    /// name, whether `get_device_and_config` takes the macOS default-device
    /// fast path or falls through to enumeration. The fast path is what makes
    /// this worth asserting: it returns `default_input_device()` rather than
    /// the enumerated match, so a mismatch would hand the recorder a different
    /// microphone than the one the user picked.
    ///
    /// Ignored for the same reason as the test above, and it says nothing at
    /// all on a machine with no inputs.
    #[test]
    #[ignore = "reads the host's real audio hardware"]
    fn every_listed_input_resolves_to_a_device_of_that_name() {
        let host = cpal::default_host();
        let listed = configure_macos_audio(&host).expect("Core Audio enumeration");
        let inputs: Vec<&AudioDevice> = listed
            .iter()
            .filter(|device| device.device_type == DeviceType::Input)
            .collect();

        if inputs.is_empty() {
            println!("no input devices attached — nothing to resolve");
            return;
        }

        let default_input = host
            .default_input_device()
            .and_then(|device| device.name().ok());
        println!("default input: {default_input:?}");

        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        for device in inputs {
            let took_fast_path = default_input.as_deref() == Some(device.name.as_str());
            let (resolved, _config) = runtime
                .block_on(crate::audio::devices::configuration::get_device_and_config(
                    device,
                ))
                .unwrap_or_else(|error| panic!("'{}' did not resolve: {error}", device.name));

            assert_eq!(
                resolved.name().ok().as_deref(),
                Some(device.name.as_str()),
                "'{}' resolved to a different device (fast path: {took_fast_path})",
                device.name
            );
        }
    }
}
