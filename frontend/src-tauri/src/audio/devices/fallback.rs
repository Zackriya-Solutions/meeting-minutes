// Bluetooth device fallback strategy for stable Core Audio recording (macOS-specific)
//
// This module implements automatic fallback to built-in devices when
// Bluetooth devices are detected as system defaults on macOS. This solves:
// - Bluetooth variable sample rate issues (Core Audio may resample dynamically)
// - Inconsistent sample rates when mixing mic + system audio streams
// - ScreenCaptureKit capturing Bluetooth-processed streams with variable timing
//
// Strategy (macOS-only):
// 1. Get system default devices (mic + speaker)
// 2. Detect if EACH is Bluetooth using InputDeviceKind::detect()
// 3. For EACH Bluetooth device detected → Override to built-in MacBook device
// 4. Return final devices with detailed rationale logging
//
// Note: Bluetooth mic and speaker are checked INDEPENDENTLY - one, both, or
// neither could be Bluetooth and need override.
//
// User still hears via Bluetooth (playback uses default), but recording
// captures via stable wired path (built-in mic + ScreenCaptureKit from built-in).

use anyhow::Result;
use log::{info, warn};

use super::configuration::AudioDevice;
use super::microphone::{default_input_device, find_builtin_input_device};
use super::speakers::default_output_device;
use crate::audio::device_detection::InputDeviceKind;

/// Get safe recording devices with automatic Bluetooth fallback (macOS-specific)
///
/// This function intelligently selects audio devices for recording on macOS:
/// - Checks microphone: if Bluetooth → override to built-in mic
/// - Checks speaker: if Bluetooth → override to built-in speaker
/// - Each device is evaluated INDEPENDENTLY
///
/// # Rationale for Bluetooth Override
///
/// Bluetooth devices on macOS can have variable sample rates as Core Audio
/// and the Bluetooth stack may resample dynamically. When ScreenCaptureKit
/// captures from a Bluetooth output device, it captures the processed stream
/// which may have inconsistent sample rates, causing sync issues when mixing
/// with the microphone stream.
///
/// Built-in devices have fixed, consistent sample rates → reliable mixing.
///
/// # Returns
///
/// Tuple of (microphone, system_audio) where:
/// - Some(device) = Device found and safe for recording
/// - None = No device available (non-fatal, recording can continue with single source)
///
/// # Example
///
/// ```rust
/// // When AirPods are default mic, built-in speaker is default output:
/// let (mic, system) = get_safe_recording_devices_macos()?;
///
/// // Logs:
/// // "🎧 Bluetooth microphone detected: AirPods Pro"
/// // "→ Overriding to stable built-in: MacBook Pro Microphone"
/// // "✅ Using wired speaker: MacBook Pro Speakers"
/// ```
#[cfg(target_os = "macos")]
pub fn get_safe_recording_devices_macos() -> Result<(Option<AudioDevice>, Option<AudioDevice>)> {
    info!("🔍 [macOS] Selecting recording devices with Bluetooth detection...");

    // Step 1: Get system defaults
    let default_mic = default_input_device().ok();
    let default_speaker = default_output_device().ok();

    // Step 2: Process microphone with Bluetooth override
    let final_mic = if let Some(ref mic) = default_mic {
        // Detect if microphone is Bluetooth
        // Use placeholder buffer_size/sample_rate (detection uses name + Core Audio API primarily)
        let device_kind = InputDeviceKind::detect(&mic.name, 512, 48000);

        if device_kind.is_bluetooth() {
            warn!("🎧 Bluetooth microphone detected: '{}'", mic.name);
            warn!("   Bluetooth introduces variable sample rates with Core Audio");

            // Try to find built-in microphone as fallback
            match find_builtin_input_device()? {
                Some(builtin_mic) => {
                    info!("→ ✅ Overriding to stable built-in microphone: '{}'", builtin_mic.name);
                    info!("   Built-in provides consistent sample rates for reliable mixing");
                    Some(builtin_mic)
                }
                None => {
                    warn!("→ ⚠️ No built-in microphone found - using Bluetooth anyway");
                    warn!("   Recording may experience sample rate sync issues");
                    warn!("   Consider using wired microphone for better stability");
                    Some(mic.clone())
                }
            }
        } else {
            // Not Bluetooth - use as-is
            info!("✅ Using wired/built-in microphone: '{}' (device type: {:?})", mic.name, device_kind);
            Some(mic.clone())
        }
    } else {
        warn!("⚠️ No default microphone found");
        None
    };

    // Step 3: Process speaker/system audio - KEEP AS-IS (macOS-specific behavior)
    // CRITICAL: On macOS, ScreenCaptureKit captures the digital audio stream being
    // sent to the output device BEFORE Bluetooth encoding happens. This means:
    // - If user has Bluetooth AirPods, audio is actively playing through them
    // - ScreenCaptureKit captures from that active output stream (pristine quality)
    // - We MUST keep the Bluetooth speaker as the system device so ScreenCaptureKit
    //   captures from where the audio is actually going
    //
    // If we override to built-in speakers when user is playing through Bluetooth,
    // ScreenCaptureKit will try to capture from built-in, but NO AUDIO IS THERE!
    let final_speaker = if let Some(ref speaker) = default_speaker {
        let device_kind = InputDeviceKind::detect(&speaker.name, 512, 48000);

        if device_kind.is_bluetooth() {
            warn!("🔊 Bluetooth speaker detected: '{}'", speaker.name);
            info!("   macOS: ScreenCaptureKit captures digital stream BEFORE Bluetooth encoding");
            info!("   Keeping Bluetooth speaker - captures from active output (pristine quality)");
            Some(speaker.clone())
        } else {
            info!("✅ Using wired/built-in speaker: '{}' (device type: {:?})", speaker.name, device_kind);
            Some(speaker.clone())
        }
    } else {
        warn!("⚠️ No default speaker found - system audio will not be recorded");
        None
    };

    // Summary logging
    match (&final_mic, &final_speaker) {
        (Some(mic), Some(speaker)) => {
            info!("📋 [macOS] Recording device selection complete:");
            info!("   Microphone: '{}'", mic.name);
            info!("   System Audio: '{}' (via ScreenCaptureKit)", speaker.name);
        }
        (Some(mic), None) => {
            info!("📋 [macOS] Recording device selection complete:");
            info!("   Microphone: '{}' (system audio unavailable)", mic.name);
        }
        (None, Some(speaker)) => {
            warn!("📋 [macOS] Recording device selection complete:");
            warn!("   System Audio: '{}' (microphone unavailable)", speaker.name);
        }
        (None, None) => {
            warn!("❌ No recording devices available - cannot start recording");
        }
    }

    Ok((final_mic, final_speaker))
}

// Non-macOS platforms: Just use system defaults (no Bluetooth override needed)
#[cfg(not(target_os = "macos"))]
pub fn get_safe_recording_devices() -> Result<(Option<AudioDevice>, Option<AudioDevice>)> {
    info!("🔍 Selecting default recording devices (no Bluetooth override on this platform)");

    let mic = default_input_device().ok();
    let speaker = default_output_device().ok();

    Ok((mic, speaker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn test_bluetooth_override_logic() {
        // This test verifies the logic but requires actual audio devices
        // Run manually on macOS development machines to verify behavior

        // Expected behavior when AirPods connected as default:
        // - Should detect Bluetooth via Core Audio API or name heuristics
        // - Should find built-in MacBook microphone
        // - Should override to built-in for recording
        // - Each device (mic and speaker) evaluated independently

        // Expected behavior when built-in mic is default:
        // - Should detect as Wired via Core Audio
        // - Should use built-in directly (no override needed)
    }
}
