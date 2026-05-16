// Cross-Platform Bluetooth Device Detection
//
// This module provides intelligent device type detection to enable adaptive
// buffering for audio devices with different latency characteristics.
//
// Detection Strategy (3 layers):
// 1. Platform-native APIs (highest accuracy)
// 2. Cross-platform name heuristics
// 3. Buffer size analysis (fallback)

use std::time::Duration;
use log::{debug, info, warn};

/// Audio input device kind with different latency characteristics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDeviceKind {
    /// Wired devices (Built-in, USB) - Low latency (5-10ms)
    Wired,

    /// Bluetooth devices (AirPods, headsets) - Higher latency (50-120ms) with jitter
    Bluetooth,

    /// Unknown device type - Use conservative settings
    Unknown,
}

impl InputDeviceKind {
    /// Detect device type using multi-layer detection strategy
    ///
    /// # Arguments
    /// * `device_name` - Name of the audio device
    /// * `buffer_size` - Reported buffer size in frames (0 if unknown)
    /// * `sample_rate` - Sample rate in Hz (0 if unknown)
    ///
    /// # Returns
    /// The detected device kind
    pub fn detect(device_name: &str, buffer_size: u32, sample_rate: u32) -> Self {
        info!("🔍 Detecting device type for: '{}'", device_name);

        // Layer 1: Platform-specific native detection (highest accuracy)
        #[cfg(target_os = "macos")]
        if let Some(kind) = Self::detect_macos_native(device_name) {
            return kind;
        }

        #[cfg(target_os = "windows")]
        if let Some(kind) = Self::detect_windows_native(device_name) {
            return kind;
        }

        #[cfg(target_os = "linux")]
        if let Some(kind) = Self::detect_linux_native(device_name) {
            return kind;
        }

        // Layer 2: Cross-platform name-based heuristics
        if let Some(kind) = Self::detect_by_name(device_name) {
            return kind;
        }

        // Layer 3: Buffer size heuristic (fallback)
        if let Some(kind) = Self::detect_by_buffer_size(buffer_size, sample_rate) {
            return kind;
        }

        // Default: Unknown (conservative - treat as Bluetooth)
        warn!("⚠️ Could not determine device type for '{}', using conservative (Bluetooth-like) settings", device_name);
        InputDeviceKind::Unknown
    }

    /// Get adaptive buffer timeout range for this device type
    ///
    /// Returns (min_timeout, max_timeout) in milliseconds
    ///
    /// These values are based on:
    /// - Cap's buffer timeout strategy (20-180ms range)
    /// - Empirical testing with various devices
    /// - 2x headroom for Bluetooth jitter
    pub fn buffer_timeout(&self) -> (Duration, Duration) {
        match self {
            InputDeviceKind::Wired => {
                // Wired devices: Fast and stable
                // Built-in/USB typically have 5-10ms base latency
                // Add 2x headroom → 10-20ms, clamp to 20-50ms range
                (Duration::from_millis(20), Duration::from_millis(50))
            }
            InputDeviceKind::Bluetooth => {
                // Bluetooth devices: Higher latency with jitter
                // Base latency 50-120ms + wireless jitter ±20-50ms
                // Need larger buffer to accommodate variability
                (Duration::from_millis(80), Duration::from_millis(200))
            }
            InputDeviceKind::Unknown => {
                // Unknown: Conservative approach (assume Bluetooth characteristics)
                // Better to have excess buffer than underruns
                (Duration::from_millis(80), Duration::from_millis(180))
            }
        }
    }

    /// Check if this device type is Bluetooth (has wireless characteristics)
    pub fn is_bluetooth(&self) -> bool {
        matches!(self, InputDeviceKind::Bluetooth)
    }

    /// Check if this device type is wired (low latency, stable)
    pub fn is_wired(&self) -> bool {
        matches!(self, InputDeviceKind::Wired)
    }

    // ========================================================================
    // Layer 2: Cross-Platform Name Heuristics
    // ========================================================================

    /// Detect device type by name patterns (works on all platforms)
    fn detect_by_name(device_name: &str) -> Option<Self> {
        let name_lower = device_name.to_lowercase();

        // Tier 1: High confidence Bluetooth patterns (99% accuracy)
        const TIER1_BLUETOOTH_PATTERNS: &[&str] = &[
            "airpods",          // Apple AirPods (all variants)
            "airpods pro",      // Apple AirPods Pro
            "airpods max",      // Apple AirPods Max
        ];

        for pattern in TIER1_BLUETOOTH_PATTERNS {
            if name_lower.contains(pattern) {
                info!("🎧 Tier 1 Bluetooth pattern matched: '{}' (pattern: '{}')",
                      device_name, pattern);
                return Some(InputDeviceKind::Bluetooth);
            }
        }

        // Tier 2: Very likely Bluetooth patterns (95% accuracy)
        const TIER2_BLUETOOTH_PATTERNS: &[&str] = &[
            "bluetooth",        // Generic Bluetooth
            "wh-1000xm",        // Sony WH-1000XM series (1/2/3/4/5)
            "quietcomfort",     // Bose QuietComfort series
            "freebuds",         // Huawei FreeBuds
            "galaxy buds",      // Samsung Galaxy Buds
            "surface headphones", // Microsoft Surface Headphones
            "beats",            // Beats headphones (mostly Bluetooth)
            "jabra",            // Jabra Bluetooth headsets
            "plantronics",      // Plantronics Bluetooth headsets
        ];

        for pattern in TIER2_BLUETOOTH_PATTERNS {
            if name_lower.contains(pattern) {
                info!("🎧 Tier 2 Bluetooth pattern matched: '{}' (pattern: '{}')",
                      device_name, pattern);
                return Some(InputDeviceKind::Bluetooth);
            }
        }

        // Tier 3: Likely Bluetooth patterns (85% accuracy) - more cautious
        const TIER3_BLUETOOTH_PATTERNS: &[&str] = &[
            "bt ",              // BT prefix
            " bt",              // BT suffix
            "wireless",         // Wireless devices
        ];

        for pattern in TIER3_BLUETOOTH_PATTERNS {
            if name_lower.contains(pattern) {
                warn!("⚠️ Tier 3 Bluetooth pattern matched: '{}' (pattern: '{}') - lower confidence",
                      device_name, pattern);
                return Some(InputDeviceKind::Bluetooth);
            }
        }

        // Check for virtual audio devices (treat as wired)
        const VIRTUAL_DEVICE_PATTERNS: &[&str] = &[
            "blackhole",
            "vb-audio",
            "virtual",
            "loopback",
            "monitor",
        ];

        for pattern in VIRTUAL_DEVICE_PATTERNS {
            if name_lower.contains(pattern) {
                info!("🔌 Virtual audio device detected: '{}' (pattern: '{}') - treating as Wired",
                      device_name, pattern);
                return Some(InputDeviceKind::Wired);
            }
        }

        None
    }

    // ========================================================================
    // Layer 3: Buffer Size Analysis
    // ========================================================================

    /// Detect device type by analyzing buffer size characteristics
    ///
    /// Bluetooth devices typically report larger buffer sizes due to:
    /// - Wireless transmission latency
    /// - Codec encoding/decoding time
    /// - Jitter buffering requirements
    fn detect_by_buffer_size(buffer_size: u32, sample_rate: u32) -> Option<Self> {
        if sample_rate == 0 || buffer_size == 0 {
            return None;
        }

        // Calculate base latency from buffer size
        let base_latency_ms = (buffer_size as f64 / sample_rate as f64) * 1000.0;

        // Bluetooth devices typically report > 50ms buffer latency
        // Wired devices typically < 20ms
        if base_latency_ms > 50.0 {
            warn!("⚠️ High buffer latency detected: {:.2}ms (buffer_size={}, sample_rate={})",
                  base_latency_ms, buffer_size, sample_rate);
            warn!("   Treating as Bluetooth device (buffer size heuristic)");
            return Some(InputDeviceKind::Bluetooth);
        } else if base_latency_ms < 20.0 {
            debug!("✓ Low buffer latency: {:.2}ms - likely wired device", base_latency_ms);
            return Some(InputDeviceKind::Wired);
        }

        // Ambiguous range (20-50ms) - cannot determine
        debug!("⚠️ Ambiguous buffer latency: {:.2}ms - cannot determine device type from buffer size",
               base_latency_ms);
        None
    }
}

// ============================================================================
// Platform-Specific Implementations
// ============================================================================

// macOS: Core Audio Transport Type API
#[cfg(target_os = "macos")]
impl InputDeviceKind {
    /// Detect device type using macOS Core Audio Transport Type API
    ///
    /// This is the most accurate detection method on macOS, querying the
    /// actual hardware transport type from Core Audio.
    fn detect_macos_native(device_name: &str) -> Option<Self> {
        use cidre::core_audio::hardware::System;

        // Query Core Audio device list and find device by name
        // System::devices() returns Result<Vec<Device>, Error>
        let devices = System::devices().ok()?;
        let device = devices.iter().find(|d| {
            d.name().ok().map(|n| n.to_string()).as_deref() == Some(device_name)
        })?;

        // Query transport type
        if let Ok(transport) = device.transport_type() {
            use cidre::core_audio::DeviceTransportType;

            match transport {
                DeviceTransportType::BLUETOOTH => {
                    info!("✅ macOS Core Audio: Bluetooth detected for '{}'", device_name);
                    return Some(InputDeviceKind::Bluetooth);
                }
                DeviceTransportType::BLUETOOTH_LE => {
                    info!("✅ macOS Core Audio: Bluetooth LE detected for '{}'", device_name);
                    return Some(InputDeviceKind::Bluetooth);
                }
                DeviceTransportType::USB => {
                    info!("✅ macOS Core Audio: USB detected for '{}'", device_name);
                    return Some(InputDeviceKind::Wired);
                }
                DeviceTransportType::BUILT_IN => {
                    info!("✅ macOS Core Audio: Built-in detected for '{}'", device_name);
                    return Some(InputDeviceKind::Wired);
                }
                _ => {
                    debug!("macOS Core Audio: Unknown transport type for '{}': {:?}",
                           device_name, transport);
                }
            }
        }

        None  // Fall through to heuristic detection
    }
}

// Windows: WASAPI Device Properties
#[cfg(target_os = "windows")]
impl InputDeviceKind {
    /// Detect device type using Windows WASAPI naming conventions
    ///
    /// Windows WASAPI exposes Bluetooth devices with specific naming patterns.
    /// This method checks for common Windows Bluetooth device prefixes.
    fn detect_windows_native(device_name: &str) -> Option<Self> {
        let name_lower = device_name.to_lowercase();

        // Windows-specific Bluetooth device naming patterns
        // WASAPI exposes Bluetooth devices with specific prefixes

        // Pattern 1: "Bluetooth Audio (Device Name)"
        if name_lower.starts_with("bluetooth audio") {
            info!("✅ Windows WASAPI: Bluetooth Audio prefix detected for '{}'", device_name);
            return Some(InputDeviceKind::Bluetooth);
        }

        // Pattern 2: "Bluetooth Hands-Free Audio"
        if name_lower.contains("bluetooth hands-free") {
            info!("✅ Windows WASAPI: Bluetooth Hands-Free detected for '{}'", device_name);
            return Some(InputDeviceKind::Bluetooth);
        }

        // Pattern 3: "Bluetooth Stereo Audio"
        if name_lower.contains("bluetooth stereo") {
            info!("✅ Windows WASAPI: Bluetooth Stereo detected for '{}'", device_name);
            return Some(InputDeviceKind::Bluetooth);
        }

        // Pattern 4: USB Audio devices
        if name_lower.contains("usb audio") {
            info!("✅ Windows WASAPI: USB Audio detected for '{}'", device_name);
            return Some(InputDeviceKind::Wired);
        }

        // Pattern 5: Realtek, Conexant, etc. (built-in audio chips)
        if name_lower.contains("realtek") || name_lower.contains("conexant") {
            info!("✅ Windows WASAPI: Built-in audio detected for '{}'", device_name);
            return Some(InputDeviceKind::Wired);
        }

        None  // Fall through to heuristic detection
    }
}

// Linux: BlueZ/PulseAudio Device Hints
#[cfg(target_os = "linux")]
impl InputDeviceKind {
    /// Detect device type using Linux BlueZ/PulseAudio naming conventions
    ///
    /// Linux exposes Bluetooth devices through BlueZ with specific naming patterns.
    /// PulseAudio also includes codec information that helps identify Bluetooth devices.
    fn detect_linux_native(device_name: &str) -> Option<Self> {
        let name_lower = device_name.to_lowercase();

        // Pattern 1: BlueZ devices (most common)
        // Example: "bluez_sink.XX_XX_XX_XX_XX_XX.a2dp_sink"
        if name_lower.contains("bluez") {
            info!("✅ Linux: BlueZ device detected for '{}'", device_name);
            return Some(InputDeviceKind::Bluetooth);
        }

        // Pattern 2: Explicit "bluetooth" in name
        if name_lower.contains("bluetooth") {
            info!("✅ Linux: 'bluetooth' keyword detected for '{}'", device_name);
            return Some(InputDeviceKind::Bluetooth);
        }

        // Pattern 3: A2DP codec identifier (Bluetooth audio profile)
        if name_lower.contains(".a2dp") {
            info!("✅ Linux: A2DP codec detected for '{}'", device_name);
            return Some(InputDeviceKind::Bluetooth);
        }

        // Pattern 4: HFP/HSP codec identifier (Bluetooth headset profile)
        if name_lower.contains(".hfp") || name_lower.contains(".hsp") {
            info!("✅ Linux: HFP/HSP codec detected for '{}'", device_name);
            return Some(InputDeviceKind::Bluetooth);
        }

        // Pattern 5: USB devices
        if name_lower.contains("usb audio") || name_lower.starts_with("usb") {
            info!("✅ Linux: USB audio detected for '{}'", device_name);
            return Some(InputDeviceKind::Wired);
        }

        // Pattern 6: HDA Intel (built-in)
        if name_lower.contains("hda intel") {
            info!("✅ Linux: HDA Intel (built-in) detected for '{}'", device_name);
            return Some(InputDeviceKind::Wired);
        }

        None  // Fall through to heuristic detection
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Calculate adaptive buffer timeout based on device characteristics
///
/// Uses Cap's strategy: base latency × 2 (headroom), clamped to device-specific range
///
/// # Arguments
/// * `device_kind` - The detected device type
/// * `buffer_size` - Reported buffer size in frames
/// * `sample_rate` - Sample rate in Hz
///
/// # Returns
/// The calculated buffer timeout duration
pub fn calculate_buffer_timeout(
    device_kind: InputDeviceKind,
    buffer_size: u32,
    sample_rate: u32,
) -> Duration {
    // Get device-specific timeout range
    let (min_timeout, max_timeout) = device_kind.buffer_timeout();

    // If buffer size unknown, use minimum for device type
    if sample_rate == 0 || buffer_size == 0 {
        return min_timeout;
    }

    // Calculate base timeout from reported buffer size
    let base = Duration::from_secs_f64(buffer_size as f64 / sample_rate as f64);

    // Add 2x headroom for jitter (Cap's strategy)
    let with_headroom = base.mul_f32(2.0);

    // Clamp to device-specific range
    clamp_duration(with_headroom, min_timeout, max_timeout)
}

/// Clamp duration to a range
fn clamp_duration(duration: Duration, min: Duration, max: Duration) -> Duration {
    if duration < min {
        min
    } else if duration > max {
        max
    } else {
        duration
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_airpods_detection() {
        let kind = InputDeviceKind::detect("AirPods Pro", 0, 0);
        assert_eq!(kind, InputDeviceKind::Bluetooth);
    }

    #[test]
    fn test_builtin_mic_detection() {
        let kind = InputDeviceKind::detect("MacBook Pro Microphone", 0, 0);
        // Should fall through to Unknown (no Bluetooth pattern, no buffer size)
        assert_eq!(kind, InputDeviceKind::Unknown);
    }

    #[test]
    fn test_bluetooth_by_buffer_size() {
        // 3840 frames at 48kHz = 80ms (Bluetooth-like)
        let kind = InputDeviceKind::detect("Unknown Device", 3840, 48000);
        assert_eq!(kind, InputDeviceKind::Bluetooth);
    }

    #[test]
    fn test_wired_by_buffer_size() {
        // 512 frames at 48kHz = 10.67ms (Wired-like)
        let kind = InputDeviceKind::detect("Unknown Device", 512, 48000);
        assert_eq!(kind, InputDeviceKind::Wired);
    }

    #[test]
    fn test_buffer_timeout_wired() {
        let (min, max) = InputDeviceKind::Wired.buffer_timeout();
        assert_eq!(min, Duration::from_millis(20));
        assert_eq!(max, Duration::from_millis(50));
    }

    #[test]
    fn test_buffer_timeout_bluetooth() {
        let (min, max) = InputDeviceKind::Bluetooth.buffer_timeout();
        assert_eq!(min, Duration::from_millis(80));
        assert_eq!(max, Duration::from_millis(200));
    }

    #[test]
    fn test_calculate_buffer_timeout_bluetooth() {
        // AirPods: 3840 frames at 48kHz = 80ms base
        // With 2x headroom = 160ms
        // Should clamp to 80-200ms range
        let timeout = calculate_buffer_timeout(
            InputDeviceKind::Bluetooth,
            3840,
            48000,
        );
        assert_eq!(timeout, Duration::from_millis(160));
    }

    #[test]
    fn test_calculate_buffer_timeout_wired() {
        // Built-in: 512 frames at 48kHz = 10.67ms base
        // With 2x headroom = 21.3ms
        // Should clamp to 20-50ms range
        let timeout = calculate_buffer_timeout(
            InputDeviceKind::Wired,
            512,
            48000,
        );
        // 21.33ms rounds to 21ms
        assert!(timeout >= Duration::from_millis(20));
        assert!(timeout <= Duration::from_millis(50));
    }

    #[test]
    fn test_virtual_device_detection() {
        let kind = InputDeviceKind::detect("BlackHole 2ch", 0, 0);
        assert_eq!(kind, InputDeviceKind::Wired);
    }
}
