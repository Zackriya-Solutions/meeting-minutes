// Audio Device Diagnostics
//
// Comprehensive logging and diagnostics for audio device capabilities
// Helps debug device detection, buffer settings, and performance issues

use cpal::SupportedStreamConfig;
use log::{info, warn};

use super::devices::AudioDevice;
use super::device_detection::{InputDeviceKind, calculate_buffer_timeout};

/// Log comprehensive device capabilities and detection results
///
/// This function provides detailed diagnostic information useful for:
/// - Debugging device detection issues
/// - Understanding platform-specific behavior
/// - Validating buffer timeout calculations
/// - Investigating performance problems
///
/// # Arguments
/// * `device` - The audio device to diagnose
/// * `config` - The supported stream configuration
/// * `detected_kind` - The detected device type
pub fn log_device_capabilities(
    device: &AudioDevice,
    config: &SupportedStreamConfig,
    detected_kind: InputDeviceKind,
) {
    // Calculate various metrics
    let sample_rate = config.sample_rate().0;
    let channels = config.channels();
    let buffer_size_enum = config.buffer_size();
    let sample_format = config.sample_format();

    // Extract buffer size value from enum
    let buffer_size_value: u32 = match buffer_size_enum {
        cpal::SupportedBufferSize::Range { min: _, max } => {
            // Use max for conservative estimate
            *max
        }
        cpal::SupportedBufferSize::Unknown => 0,
    };

    // Calculate buffer latency
    let buffer_latency_ms = if sample_rate > 0 && buffer_size_value > 0 {
        (buffer_size_value as f64 / sample_rate as f64) * 1000.0
    } else {
        0.0
    };

    // Get adaptive timeout range
    let (min_timeout, max_timeout) = detected_kind.buffer_timeout();
    let min_timeout_ms = min_timeout.as_secs_f64() * 1000.0;
    let max_timeout_ms = max_timeout.as_secs_f64() * 1000.0;

    // Calculate actual buffer timeout that will be used
    let actual_timeout = calculate_buffer_timeout(detected_kind, buffer_size_value, sample_rate);
    let actual_timeout_ms = actual_timeout.as_secs_f64() * 1000.0;

    // Print diagnostic box
    info!("╔═══════════════════════════════════════════════════════════╗");
    info!("║ Audio Device Diagnostics                                  ║");
    info!("╠═══════════════════════════════════════════════════════════╣");
    info!("  Platform:          {}", std::env::consts::OS);
    info!("  Device Name:       {}", device.name);
    info!("  Device Type:       {:?}", device.device_type);
    info!("  Detected Kind:     {:?}", detected_kind);
    info!("  ───────────────────────────────────────────────────────────");
    info!("  Sample Rate:       {} Hz", sample_rate);
    info!("  Channels:          {}", channels);
    info!("  Buffer Size:       {} frames", buffer_size_value);
    info!("  Sample Format:     {:?}", sample_format);
    info!("  ───────────────────────────────────────────────────────────");
    info!("  Buffer Latency:    {:.2}ms", buffer_latency_ms);
    info!("  Timeout Range:     {:.0}ms - {:.0}ms", min_timeout_ms, max_timeout_ms);
    info!("  Actual Timeout:    {:.0}ms", actual_timeout_ms);
    info!("  ───────────────────────────────────────────────────────────");

    // Add platform-specific diagnostics
    #[cfg(target_os = "macos")]
    log_macos_specific_info(device, config);

    #[cfg(target_os = "windows")]
    log_windows_specific_info(device, config);

    #[cfg(target_os = "linux")]
    log_linux_specific_info(device, config);

    info!("╚═══════════════════════════════════════════════════════════╝");

    // Add warnings for potential issues
    check_for_issues(detected_kind, buffer_latency_ms, sample_rate, channels);
}

/// Check for potential configuration issues and log warnings
fn check_for_issues(
    detected_kind: InputDeviceKind,
    buffer_latency_ms: f64,
    sample_rate: u32,
    channels: u16,
) {
    // Issue 1: Bluetooth device with very low buffer latency
    if detected_kind.is_bluetooth() && buffer_latency_ms < 50.0 {
        warn!("⚠️ POTENTIAL ISSUE: Bluetooth device has unusually low buffer latency ({:.2}ms)",
              buffer_latency_ms);
        warn!("   This may cause buffer underruns. Expect gaps or audio dropouts.");
        warn!("   The adaptive mixer will compensate with larger timeout ({:.0}ms).",
              detected_kind.buffer_timeout().1.as_secs_f64() * 1000.0);
    }

    // Issue 2: Wired device with very high buffer latency
    if detected_kind.is_wired() && buffer_latency_ms > 50.0 {
        warn!("⚠️ POTENTIAL ISSUE: Wired device has unusually high buffer latency ({:.2}ms)",
              buffer_latency_ms);
        warn!("   This is unexpected for wired devices. May be misconfigured.");
    }

    // Issue 3: Non-standard sample rate
    if sample_rate != 48000 && sample_rate != 0 {
        info!("ℹ️ NOTE: Device uses non-standard sample rate ({} Hz)", sample_rate);
        info!("   Will be resampled to 48000 Hz for processing.");
    }

    // Issue 4: Stereo input (will be converted to mono)
    if channels > 1 {
        info!("ℹ️ NOTE: Device has {} channels (stereo/multi-channel)", channels);
        info!("   Will be converted to mono for processing.");
    }
}

/// macOS-specific diagnostic information
#[cfg(target_os = "macos")]
fn log_macos_specific_info(_device: &AudioDevice, _config: &SupportedStreamConfig) {
    use cidre::core_audio::hardware::System;

    info!("  macOS-Specific:");

    // Try to get Core Audio device info
    // System::devices() returns Result<Vec<Device>, Error>
    if let Ok(devices) = System::devices() {
        if let Some(ca_device) = devices.iter().find(|d| {
            d.name().ok().map(|n| n.to_string()).as_deref() == Some(_device.name.as_str())
        }) {
            // Get transport type
            if let Ok(transport) = ca_device.transport_type() {
                info!("    Transport Type:  {:?}", transport);
            }

            // Get device manufacturer
            if let Ok(manufacturer) = ca_device.manufacturer() {
                info!("    Manufacturer:    {}", manufacturer.to_string());
            }

            // Get device UID
            if let Ok(uid) = ca_device.uid() {
                info!("    Device UID:      {}", uid.to_string());
            }
        }
    }
}

/// Windows-specific diagnostic information
#[cfg(target_os = "windows")]
fn log_windows_specific_info(device: &AudioDevice, _config: &SupportedStreamConfig) {
    info!("  Windows-Specific:");
    info!("    Device Name:     {}", device.name);

    // Check if WASAPI naming patterns detected
    let name_lower = device.name.to_lowercase();
    if name_lower.contains("bluetooth") {
        info!("    WASAPI Type:     Bluetooth (detected from name)");
    } else if name_lower.contains("usb") {
        info!("    WASAPI Type:     USB");
    } else if name_lower.contains("realtek") || name_lower.contains("conexant") {
        info!("    WASAPI Type:     Built-in Audio Chip");
    }
}

/// Linux-specific diagnostic information
#[cfg(target_os = "linux")]
fn log_linux_specific_info(device: &AudioDevice, _config: &SupportedStreamConfig) {
    info!("  Linux-Specific:");
    info!("    Device Name:     {}", device.name);

    // Check for BlueZ/PulseAudio patterns
    let name_lower = device.name.to_lowercase();
    if name_lower.contains("bluez") {
        info!("    Audio Stack:     BlueZ (Bluetooth)");
        if name_lower.contains(".a2dp") {
            info!("    Bluetooth Codec: A2DP (Advanced Audio Distribution Profile)");
        } else if name_lower.contains(".hfp") || name_lower.contains(".hsp") {
            info!("    Bluetooth Codec: HFP/HSP (Headset Profile)");
        }
    } else if name_lower.contains("pulse") || name_lower.contains("monitor") {
        info!("    Audio Stack:     PulseAudio");
    } else if name_lower.contains("alsa") || name_lower.contains("hda") {
        info!("    Audio Stack:     ALSA");
    }
}

/// Log a concise device detection summary (for quick debugging)
pub fn log_detection_summary(
    device_name: &str,
    detected_kind: InputDeviceKind,
    buffer_size: u32,
    sample_rate: u32,
) {
    let (min_ms, max_ms) = detected_kind.buffer_timeout();
    let min_ms_val = min_ms.as_secs_f64() * 1000.0;
    let max_ms_val = max_ms.as_secs_f64() * 1000.0;

    info!("📊 Device '{}': {:?} → Timeout: {:.0}-{:.0}ms (buffer: {}@{}Hz)",
          device_name,
          detected_kind,
          min_ms_val,
          max_ms_val,
          buffer_size,
          sample_rate);
}

/// Log buffer health statistics during recording
pub fn log_buffer_health(
    device_name: &str,
    device_kind: InputDeviceKind,
    current_buffer_size: usize,
    max_buffer_size: usize,
    dropped_frames: u64,
) {
    let buffer_utilization = (current_buffer_size as f64 / max_buffer_size as f64) * 100.0;

    if buffer_utilization > 80.0 {
        warn!("⚠️ HIGH BUFFER UTILIZATION: '{}' ({:?})", device_name, device_kind);
        warn!("   Current: {} / {} samples ({:.1}%)",
              current_buffer_size, max_buffer_size, buffer_utilization);
        warn!("   Dropped frames: {}", dropped_frames);

        if device_kind.is_bluetooth() {
            warn!("   This is a Bluetooth device - connection quality may be poor");
            warn!("   Consider moving closer to reduce wireless interference");
        }
    } else if current_buffer_size == 0 && dropped_frames > 0 {
        warn!("⚠️ BUFFER UNDERRUN: '{}' ({:?})", device_name, device_kind);
        warn!("   Dropped frames: {}", dropped_frames);
    }
}

/// Log FFmpeg mixer status
pub fn log_mixer_status(
    mic_buffered: usize,
    system_buffered: usize,
    gaps_detected: u32,
    silence_inserted_ms: f64,
) {
    info!("🎛️ Mixer Status:");
    info!("   Mic buffer:        {} samples", mic_buffered);
    info!("   System buffer:     {} samples", system_buffered);
    info!("   Gaps detected:     {}", gaps_detected);
    info!("   Silence inserted:  {:.1}ms total", silence_inserted_ms);
}

/// Log performance metrics summary
pub fn log_performance_summary(
    total_chunks_processed: u64,
    average_latency_ms: f64,
    buffer_overflows: u32,
    device_reconnects: u32,
) {
    info!("╔═══════════════════════════════════════════════════════════╗");
    info!("║ Recording Session Performance Summary                     ║");
    info!("╠═══════════════════════════════════════════════════════════╣");
    info!("  Chunks Processed:    {}", total_chunks_processed);
    info!("  Average Latency:     {:.1}ms", average_latency_ms);
    info!("  Buffer Overflows:    {} {}",
          buffer_overflows,
          if buffer_overflows == 0 { "✓" } else { "⚠️" });
    info!("  Device Reconnects:   {}", device_reconnects);
    info!("╚═══════════════════════════════════════════════════════════╝");

    if buffer_overflows > 0 {
        warn!("⚠️ Buffer overflows detected! This may indicate:");
        warn!("   1. Bluetooth connection quality issues");
        warn!("   2. System under high CPU load");
        warn!("   3. Buffer timeout settings need adjustment");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::devices::DeviceType;
    use cpal::SampleFormat;

    #[test]
    fn test_diagnostics_dont_panic() {
        // Create mock device and config
        let device = AudioDevice::new("Test Device".to_string(), DeviceType::Input);

        // Create a mock config (this is simplified - real configs are more complex)
        // Just ensure the diagnostic functions don't panic
        let detected_kind = InputDeviceKind::Wired;

        log_detection_summary("Test Device", detected_kind, 512, 48000);
        log_buffer_health("Test Device", detected_kind, 100, 1000, 0);
        log_mixer_status(500, 500, 0, 0.0);
        log_performance_summary(1000, 50.0, 0, 0);

        // If we get here without panicking, test passes
    }
}
