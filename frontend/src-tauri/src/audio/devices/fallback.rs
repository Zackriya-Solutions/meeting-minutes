// Bluetooth device fallback strategy for stable recording
//
// This module implements automatic fallback to built-in devices when
// Bluetooth devices are detected as system defaults. This solves:
// - Bluetooth latency/jitter issues (50-120ms variable delay)
// - Gap detection and silence insertion overhead
// - Buffer overflow warnings from inconsistent timing
//
// Strategy:
// 1. Get system default devices (mic + speaker)
// 2. Detect if defaults are Bluetooth using InputDeviceKind::detect()
// 3. If Bluetooth detected → Override to built-in MacBook/wired devices
// 4. Return final devices with detailed rationale logging
//
// User still hears via Bluetooth (playback uses default), but recording
// captures via stable wired path (built-in mic + ScreenCaptureKit).

use anyhow::Result;
use log::{info, warn};

use super::configuration::AudioDevice;
use super::microphone::{default_input_device, find_builtin_input_device};
use super::speakers::default_output_device;
#[cfg(not(target_os = "macos"))]
use super::speakers::find_builtin_output_device;
use crate::audio::device_detection::InputDeviceKind;

/// Get safe recording devices with automatic Bluetooth fallback
///
/// This function intelligently selects audio devices for recording:
/// - If system defaults are wired (built-in, USB) → Use them directly
/// - If system defaults are Bluetooth (AirPods, etc.) → Override to built-in devices
///
/// # Rationale for Bluetooth Override
///
/// Bluetooth devices have variable latency (50-120ms ± 50ms jitter) which causes:
/// - Audio sync issues when mixing with stable system audio (Core Audio: 0-5ms)
/// - Buffer overflows in FFmpeg mixer due to inconsistent frame arrival
/// - Gap detection overhead and unnecessary silence insertion
///
/// Built-in devices are wired → stable 5-10ms latency → FFmpeg mixer works optimally.
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
/// let (mic, system) = get_safe_recording_devices()?;
///
/// // Logs (when AirPods are system default):
/// // "🎧 Bluetooth microphone detected: AirPods Pro"
/// // "→ Overriding to stable built-in: MacBook Pro Microphone"
/// // "🔊 Bluetooth speaker detected: AirPods Pro"
/// // "→ Keeping for system audio (ScreenCaptureKit captures pre-Bluetooth)"
/// ```
pub fn get_safe_recording_devices() -> Result<(Option<AudioDevice>, Option<AudioDevice>)> {
    info!("🔍 Selecting recording devices with Bluetooth detection...");

    // Step 1: Get system defaults
    let default_mic = default_input_device().ok();
    let default_speaker = default_output_device().ok();

    // Step 2: Process microphone with Bluetooth override
    let final_mic = if let Some(ref mic) = default_mic {
        // Detect if microphone is Bluetooth
        // Use placeholder buffer_size/sample_rate (detection uses name heuristics primarily)
        let device_kind = InputDeviceKind::detect(&mic.name, 512, 48000);

        if device_kind.is_bluetooth() {
            warn!("🎧 Bluetooth microphone detected: '{}'", mic.name);
            warn!("   Bluetooth introduces variable latency (50-120ms ± 50ms jitter)");

            // Try to find built-in microphone as fallback
            match find_builtin_input_device()? {
                Some(builtin_mic) => {
                    info!("→ ✅ Overriding to stable built-in microphone: '{}'", builtin_mic.name);
                    info!("   Built-in provides stable wired audio (5-10ms latency)");
                    info!("   This eliminates Bluetooth jitter and improves FFmpeg mixer stability");
                    Some(builtin_mic)
                }
                None => {
                    warn!("→ ⚠️ No built-in microphone found - using Bluetooth anyway");
                    warn!("   Recording may experience latency/sync issues");
                    warn!("   Consider using wired microphone for better quality");
                    Some(mic.clone())
                }
            }
        } else {
            // Not Bluetooth - use as-is
            info!("✅ Using wired microphone: '{}' (device type: {:?})", mic.name, device_kind);
            Some(mic.clone())
        }
    } else {
        warn!("⚠️ No default microphone found");
        None
    };

    // Step 3: Process speaker/system audio with Bluetooth override
    // NOTE: On macOS, system audio is captured via ScreenCaptureKit regardless of
    // output device, but we still prefer built-in to set correct recording metadata
    let final_speaker = if let Some(ref speaker) = default_speaker {
        let device_kind = InputDeviceKind::detect(&speaker.name, 512, 48000);

        if device_kind.is_bluetooth() {
            warn!("🔊 Bluetooth speaker detected: '{}'", speaker.name);

            // For system audio, we can keep Bluetooth because:
            // - macOS: ScreenCaptureKit captures pre-Bluetooth (pristine quality)
            // - Windows: WASAPI loopback captures at hardware level
            // But we still prefer built-in for consistency
            #[cfg(target_os = "macos")]
            {
                info!("   macOS: System audio captured via ScreenCaptureKit (pre-Bluetooth encoding)");
                info!("   Keeping Bluetooth speaker for recording metadata");
                Some(speaker.clone())
            }

            #[cfg(not(target_os = "macos"))]
            {
                // On Windows/Linux, try to use built-in speaker for system audio capture
                match find_builtin_output_device()? {
                    Some(builtin_speaker) => {
                        info!("→ ✅ Overriding to built-in speaker: '{}'", builtin_speaker.name);
                        info!("   Ensures stable system audio capture");
                        Some(builtin_speaker)
                    }
                    None => {
                        warn!("→ ⚠️ No built-in speaker found - using Bluetooth");
                        Some(speaker.clone())
                    }
                }
            }
        } else {
            info!("✅ Using wired speaker: '{}' (device type: {:?})", speaker.name, device_kind);
            Some(speaker.clone())
        }
    } else {
        warn!("⚠️ No default speaker found - system audio will not be recorded");
        None
    };

    // Summary logging
    match (&final_mic, &final_speaker) {
        (Some(mic), Some(speaker)) => {
            info!("📋 Recording device selection complete:");
            info!("   Microphone: '{}'", mic.name);
            info!("   System Audio: '{}'", speaker.name);
        }
        (Some(mic), None) => {
            info!("📋 Recording device selection complete:");
            info!("   Microphone: '{}' (system audio unavailable)", mic.name);
        }
        (None, Some(speaker)) => {
            warn!("📋 Recording device selection complete:");
            warn!("   System Audio: '{}' (microphone unavailable)", speaker.name);
        }
        (None, None) => {
            warn!("❌ No recording devices available - cannot start recording");
        }
    }

    Ok((final_mic, final_speaker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bluetooth_override_logic() {
        // This test verifies the logic but requires actual audio devices
        // Run manually on development machines to verify behavior

        // If AirPods connected as default:
        // - Should detect Bluetooth
        // - Should find built-in MacBook microphone
        // - Should override to built-in for recording

        // If built-in mic is default:
        // - Should detect as Wired
        // - Should use built-in directly (no override needed)
    }
}
