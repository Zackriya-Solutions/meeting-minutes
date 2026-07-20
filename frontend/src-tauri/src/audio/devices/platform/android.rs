use anyhow::Result;
use cpal::traits::HostTrait;

use crate::audio::devices::configuration::{AudioDevice, DeviceType};

/// Configure Android audio devices using AAudio/OpenSL ES (cpal's oboe backend)
///
/// Android only exposes microphone input. Capturing other apps' audio is not
/// permitted by the platform, so no system (Output) devices are listed here —
/// meeting audio is picked up through the microphone instead.
///
/// Deliberately does NOT call `host.input_devices()` / `host.devices()`: cpal's
/// oboe backend implements those via `oboe::AudioDeviceInfo::request(...)`, a
/// JNI round-trip into `android.media.AudioManager` that has been observed to
/// hang indefinitely on-device (never returning, so callers relying on it never
/// see an error either — see `get_device_and_config`'s matching Android branch).
/// A phone has exactly one microphone as far as this app is concerned, and
/// cpal's placeholder "default" device (returned instantly, no JNI call) is all
/// that's needed to open it.
pub fn configure_android_audio(host: &cpal::Host) -> Result<Vec<AudioDevice>> {
    let mut devices = Vec::new();

    if host.default_input_device().is_some() {
        devices.push(AudioDevice::new("default".to_string(), DeviceType::Input));
    }

    Ok(devices)
}
