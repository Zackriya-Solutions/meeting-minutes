use anyhow::{anyhow, Result};
use cpal::traits::{HostTrait, DeviceTrait};
use log::{debug, info, warn};

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

/// Get the default system audio capture device for Linux
///
/// On Linux, system audio capture works via PulseAudio/PipeWire monitor sources.
/// Monitor sources have names ending in ".monitor" and capture audio playing through
/// the corresponding output sink.
///
/// This function:
/// 1. First tries to get monitors from pactl (PulseAudio/PipeWire)
/// 2. Falls back to ALSA loopback devices
///
/// Priority order:
/// 1. Built-in audio monitor (pci-*)
/// 2. Any other PulseAudio monitor
/// 3. ALSA loopback device
#[cfg(target_os = "linux")]
pub fn default_system_audio_device() -> Result<AudioDevice> {
    use std::process::Command;

    info!("[Linux] Looking for default system audio device (monitor source)...");

    let mut builtin_monitor: Option<String> = None;
    let mut any_monitor: Option<String> = None;

    // First, try to get PulseAudio/PipeWire monitors via pactl
    if let Ok(output) = Command::new("pactl")
        .args(["list", "sources", "short"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 2 {
                    let source_name = parts[1].trim();
                    if source_name.ends_with(".monitor") {
                        debug!("[Linux] Found PulseAudio monitor: {}", source_name);

                        // Prioritize built-in audio (pci-based)
                        if source_name.contains("pci-") && builtin_monitor.is_none() {
                            info!("[Linux] Found built-in audio monitor: {}", source_name);
                            builtin_monitor = Some(source_name.to_string());
                        } else if any_monitor.is_none() {
                            info!("[Linux] Found monitor source: {}", source_name);
                            any_monitor = Some(source_name.to_string());
                        }
                    }
                }
            }
        }
    }

    // Return PulseAudio monitor if found
    if let Some(name) = builtin_monitor {
        info!("[Linux] Using built-in audio monitor as default system audio: {}", name);
        return Ok(AudioDevice::new(name, DeviceType::Output));
    }

    if let Some(name) = any_monitor {
        info!("[Linux] Using monitor source as default system audio: {}", name);
        return Ok(AudioDevice::new(name, DeviceType::Output));
    }

    // Fall back to ALSA loopback devices from cpal
    let host = cpal::default_host();
    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            if name.contains("Loopback") || name.to_lowercase().contains("loopback") {
                info!("[Linux] Using ALSA loopback as default system audio: {}", name);
                return Ok(AudioDevice::new(name, DeviceType::Output));
            }
        }
    }

    warn!("[Linux] No monitor source found for system audio capture!");
    warn!("[Linux] Ensure PulseAudio/PipeWire is running. Run 'pactl list sources short' to check.");
    Err(anyhow!("No system audio device (monitor source) found on Linux"))
}