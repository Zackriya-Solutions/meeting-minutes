use std::time::Duration;
use log::{debug, info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDeviceKind {
    Wired,
    Bluetooth,
    Unknown,
}

impl InputDeviceKind {
    pub fn detect(device_name: &str, buffer_size: u32, sample_rate: u32) -> Self {
        info!("🔍 Detecting device type for: '{}'", device_name);

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

        if let Some(kind) = Self::detect_by_name(device_name) {
            return kind;
        }

        if let Some(kind) = Self::detect_by_buffer_size(buffer_size, sample_rate) {
            return kind;
        }

        warn!("⚠️ Could not determine device type for '{}', attempting to use potential conservative settings", device_name);
        
        if !device_name.contains("microphone") {
            warn!("⚠️ 'Unknown' device treated conservatively with Bluetooth-like settings.");
            return InputDeviceKind::Unknown;
        }
        
        InputDeviceKind::Wired
    }

    pub fn buffer_timeout(&self) -> (Duration, Duration) {
        match self {
            InputDeviceKind::Wired => (Duration::from_millis(20), Duration::from_millis(50)),
            InputDeviceKind::Bluetooth => (Duration::from_millis(80), Duration::from_millis(200)),
            InputDeviceKind::Unknown => (Duration::from_millis(80), Duration::from_millis(180)),
        }
    }

    pub fn is_bluetooth(&self) -> bool {
        matches!(self, InputDeviceKind::Bluetooth)
    }

    pub fn is_wired(&self) -> bool {
        matches!(self, InputDeviceKind::Wired)
    }

    fn detect_by_name(device_name: &str) -> Option<Self> {
        let name_lower = device_name.to_lowercase();

        const TIER1_BLUETOOTH_PATTERNS: &[&str] = &[
            "airpods",
            "airpods pro",
            "airpods max",
        ];

        for pattern in TIER1_BLUETOOTH_PATTERNS {
            if name_lower.contains(pattern) {
                info!("🎧 Tier 1 Bluetooth pattern matched: '{}' (pattern: '{}')", device_name, pattern);
                return Some(InputDeviceKind::Bluetooth);
            }
        }

        const TIER2_BLUETOOTH_PATTERNS: &[&str] = &[
            "bluetooth",
            "wh-1000xm",
            "quietcomfort",
            "freebuds",
            "galaxy buds",
            "surface headphones",
            "beats",
            "jabra",
            "plantronics",
        ];

        for pattern in TIER2_BLUETOOTH_PATTERNS {
            if name_lower.contains(pattern) {
                info!("🎧 Tier 2 Bluetooth pattern matched: '{}' (pattern: '{}')", device_name, pattern);
                return Some(InputDeviceKind::Bluetooth);
            }
        }

        const TIER3_BLUETOOTH_PATTERNS: &[&str] = &[
            "bt ",
            " bt",
            "wireless",
        ];

        for pattern in TIER3_BLUETOOTH_PATTERNS {
            if name_lower.contains(pattern) {
                warn!("⚠️ Tier 3 Bluetooth pattern matched: '{}' (pattern: '{}') - lower confidence", device_name, pattern);
                return Some(InputDeviceKind::Bluetooth);
            }
        }

        const VIRTUAL_DEVICE_PATTERNS: &[&str] = &[
            "blackhole",
            "vb-audio",
            "virtual",
            "loopback",
            "monitor",
        ];

        for pattern in VIRTUAL_DEVICE_PATTERNS {
            if name_lower.contains(pattern) {
                info!("🔌 Virtual audio device detected: '{}' (pattern: '{}') - treating as Wired", device_name, pattern);
                return Some(InputDeviceKind::Wired);
            }
        }

        None
    }

    fn detect_by_buffer_size(buffer_size: u32, sample_rate: u32) -> Option<Self> {
        if sample_rate == 0 || buffer_size == 0 {
            return None;
        }

        let base_latency_ms = (buffer_size as f64 / sample_rate as f64) * 1000.0;

        if base_latency_ms > 50.0 {
            warn!("⚠️ High buffer latency detected: {:.2}ms (buffer_size={}, sample_rate={})", base_latency_ms, buffer_size, sample_rate);
            return Some(InputDeviceKind::Bluetooth);
        } else if base_latency_ms < 20.0 {
            debug!("✓ Low buffer latency: {:.2}ms - likely wired device", base_latency_ms);
            return Some(InputDeviceKind::Wired);
        }

        debug!("⚠️ Ambiguous buffer latency: {:.2}ms - cannot determine device type from buffer size", base_latency_ms);
        None
    }
}

#[cfg(target_os = "macos")]
impl InputDeviceKind {
    fn detect_macos_native(device_name: &str) -> Option<Self> {
        use cidre::core_audio::hardware::System;

        let devices = System::devices().ok()?;
        let device = devices.iter().find(|d| {
            d.name().ok().map(|n| n.to_string()).as_deref() == Some(device_name)
        })?;

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
                    debug!("macOS Core Audio: Unknown transport type for '{}': {:?}", device_name, transport);
                }
            }
        }

        None
    }
}

#[cfg(target_os = "windows")]
impl InputDeviceKind {
    fn detect_windows_native(device_name: &str) -> Option<Self> {
        let name_lower = device_name.to_lowercase();

        if name_lower.starts_with("bluetooth audio") {
            info!("✅ Windows WASAPI: Bluetooth Audio prefix detected for '{}'", device_name);
            return Some(InputDeviceKind::Bluetooth);
        }

        if name_lower.contains("bluetooth hands-free") {
            info!("✅ Windows WASAPI: Bluetooth Hands-Free detected for '{}'", device_name);
            return Some(InputDeviceKind::Bluetooth);
        }

        if name_lower.contains("bluetooth stereo") {
            info!("✅ Windows WASAPI: Bluetooth Stereo detected for '{}'", device_name);
            return Some(InputDeviceKind::Bluetooth);
        }

        if name_lower.contains("usb audio") {
            info!("✅ Windows WASAPI: USB Audio detected for '{}'", device_name);
            return Some(InputDeviceKind::Wired);
        }

        if name_lower.contains("realtek") || name_lower.contains("conexant") {
            info!("✅ Windows WASAPI: Built-in audio detected for '{}'", device_name);
            return Some(InputDeviceKind::Wired);
        }

        None
    }
}

#[cfg(target_os = "linux")]
impl InputDeviceKind {
    fn detect_linux_native(device_name: &str) -> Option<Self> {
        let name_lower = device_name.to_lowercase();

        if name_lower.contains("bluez") {
            info!("✅ Linux: BlueZ device detected for '{}'", device_name);
            return Some(InputDeviceKind::Bluetooth);
        }

        if name_lower.contains("bluetooth") {
            info!("✅ Linux: 'bluetooth' keyword detected for '{}'", device_name);
            return Some(InputDeviceKind::Bluetooth);
        }

        if name_lower.contains(".a2dp") {
            info!("✅ Linux: A2DP codec detected for '{}'", device_name);
            return Some(InputDeviceKind::Bluetooth);
        }

        if name_lower.contains(".hfp") || name_lower.contains(".hsp") {
            info!("✅ Linux: HFP/HSP codec detected for '{}'", device_name);
            return Some(InputDeviceKind::Bluetooth);
        }

        if name_lower.contains("usb audio") || name_lower.starts_with("usb") {
            info!("✅ Linux: USB audio detected for '{}'", device_name);
            return Some(InputDeviceKind::Wired);
        }

        if name_lower.contains("hda intel") {
            info!("✅ Linux: HDA Intel (built-in) detected for '{}'", device_name);
            return Some(InputDeviceKind::Wired);
        }

        None
    }
}

pub fn calculate_buffer_timeout(device_kind: InputDeviceKind, buffer_size: u32, sample_rate: u32) -> Duration {
    let (min_timeout, max_timeout) = device_kind.buffer_timeout();

    if sample_rate == 0 || buffer_size == 0 {
        return min_timeout;
    }

    let base = Duration::from_secs_f64(buffer_size as f64 / sample_rate as f64);
    let with_headroom = base.mul_f32(2.0);
    clamp_duration(with_headroom, min_timeout, max_timeout)
}

fn clamp_duration(duration: Duration, min: Duration, max: Duration) -> Duration {
    if duration < min {
        min
    } else if duration > max {
        max
    } else {
        duration
    }
}

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
        assert_eq!(kind, InputDeviceKind::Wired);
    }

    #[test]
    fn test_bluetooth_by_buffer_size() {
        let kind = InputDeviceKind::detect("Unknown Device", 3840, 48000);
        assert_eq!(kind, InputDeviceKind::Bluetooth);
    }

    #[test]
    fn test_wired_by_buffer_size() {
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
        let timeout = calculate_buffer_timeout(InputDeviceKind::Bluetooth, 3840, 48000);
        assert_eq!(timeout, Duration::from_millis(160));
    }

    #[test]
    fn test_calculate_buffer_timeout_wired() {
        let timeout = calculate_buffer_timeout(InputDeviceKind::Wired, 512, 48000);
        assert!(timeout >= Duration::from_millis(20));
        assert!(timeout <= Duration::from_millis(50));
    }

    #[test]
    fn test_virtual_device_detection() {
        let kind = InputDeviceKind::detect("BlackHole 2ch", 0, 0);
        assert_eq!(kind, InputDeviceKind::Wired);
    }
}