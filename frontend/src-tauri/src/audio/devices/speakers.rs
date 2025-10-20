use anyhow::{anyhow, Result};
use cpal::traits::{HostTrait, DeviceTrait};
use log::{info, warn};

use super::configuration::{AudioDevice, DeviceType};

/// Get the default output (speaker/system audio) device for the system
pub fn default_output_device() -> Result<AudioDevice> {
    #[cfg(target_os = "macos")]
    {
        // Use default host for all macOS devices
        // Core Audio backend uses direct cidre API for system capture, not cpal
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("No default output device found"))?;
        return Ok(AudioDevice::new(device.name()?, DeviceType::Output));
    }

    #[cfg(target_os = "windows")]
    {
        // Try WASAPI host first for Windows
        if let Ok(wasapi_host) = cpal::host_from_id(cpal::HostId::Wasapi) {
            if let Some(device) = wasapi_host.default_output_device() {
                if let Ok(name) = device.name() {
                    return Ok(AudioDevice::new(name, DeviceType::Output));
                }
            }
        }
        // Fallback to default host if WASAPI fails
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("No default output device found"))?;
        return Ok(AudioDevice::new(device.name()?, DeviceType::Output));
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("No default output device found"))?;
        return Ok(AudioDevice::new(device.name()?, DeviceType::Output));
    }
}

/// Find the built-in speaker/output device (wired, stable, consistent sample rate)
///
/// Searches for MacBook/built-in speaker patterns to find the hardware
/// speakers instead of Bluetooth devices. This is useful for:
/// - System audio capture using ScreenCaptureKit (macOS) with consistent sample rates
/// - Getting audio before Bluetooth processing (pristine quality)
/// - Fallback when Bluetooth device is default but causes sample rate issues
///
/// Note: On macOS, system audio is captured via ScreenCaptureKit from the
/// output device. Using built-in speakers ensures Core Audio provides
/// consistent sample rates for reliable mixing with microphone.
///
/// Returns None if no built-in speaker found
pub fn find_builtin_output_device() -> Result<Option<AudioDevice>> {
    let host = cpal::default_host();

    // Built-in speaker name patterns (platform-specific)
    let builtin_patterns = [
        // macOS patterns
        "macbook",
        "built-in speakers",
        "built-in output",
        "internal speakers",
        // Windows patterns
        "speakers",
        "realtek",
        "conexant",
        "high definition audio",
        // Linux patterns
        "hda intel",
        "built-in audio",
        "analog output",
    ];

    // Search all output devices for built-in pattern matches
    for device in host.output_devices()? {
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

                    // Additional filter: exclude virtual audio devices
                    // (we want real hardware speakers for ScreenCaptureKit)
                    if name_lower.contains("blackhole") ||
                       name_lower.contains("vb-audio") ||
                       name_lower.contains("virtual") ||
                       name_lower.contains("loopback") {
                        continue; // Skip virtual devices
                    }

                    info!("🔊 Found built-in speaker: '{}'", name);
                    return Ok(Some(AudioDevice::new(name, DeviceType::Output)));
                }
            }
        }
    }

    warn!("⚠️ No built-in speaker found (searched {} patterns)", builtin_patterns.len());
    Ok(None)
}