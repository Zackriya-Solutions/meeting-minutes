use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};
use log::{debug, info, warn};
use std::collections::HashMap;
use std::process::Command;

use crate::audio::devices::configuration::{AudioDevice, DeviceType};

/// Read friendly names from /proc/asound/cards
fn get_card_friendly_names() -> HashMap<String, String> {
    let mut names = HashMap::new();

    if let Ok(content) = std::fs::read_to_string("/proc/asound/cards") {
        for line in content.lines() {
            // Parse lines like: " 1 [Wireless       ]: USB-Audio - JBL Quantum 910 Wireless"
            let trimmed = line.trim();
            if let Some(bracket_start) = trimmed.find('[') {
                if let Some(bracket_end) = trimmed.find(']') {
                    let card_id = trimmed[bracket_start + 1..bracket_end].trim().to_string();
                    // Get friendly name after the " - "
                    if let Some(dash_pos) = trimmed.find(" - ") {
                        let friendly_name = trimmed[dash_pos + 3..].trim().to_string();
                        names.insert(card_id, friendly_name);
                    }
                }
            }
        }
    }

    names
}

/// Convert ALSA device name to friendly name
fn make_friendly_name(alsa_name: &str, card_names: &HashMap<String, String>) -> String {
    // Extract card name from formats like "hw:CARD=Wireless,DEV=0" or "plughw:CARD=PCH,DEV=1"
    if let Some(card_start) = alsa_name.find("CARD=") {
        let after_card = &alsa_name[card_start + 5..];
        let card_id = if let Some(comma_pos) = after_card.find(',') {
            &after_card[..comma_pos]
        } else {
            after_card
        };

        if let Some(friendly) = card_names.get(card_id) {
            // Return friendly name with device info
            return format!("{} ({})", friendly, alsa_name);
        }
    }

    // Fallback to original name
    alsa_name.to_string()
}

/// Get PulseAudio/PipeWire monitor sources via pactl command
/// Returns a list of (source_name, description) tuples for monitor sources
fn get_pulseaudio_monitors() -> Vec<(String, String)> {
    let mut monitors = Vec::new();

    // Try to get sources from pactl (works with PulseAudio and PipeWire)
    let output = match Command::new("pactl")
        .args(["list", "sources", "short"])
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            debug!("[Linux Audio] Failed to run pactl: {}", e);
            return monitors;
        }
    };

    if !output.status.success() {
        debug!("[Linux Audio] pactl failed with status: {}", output.status);
        return monitors;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        // Format: ID<tab>NAME<tab>MODULE<tab>FORMAT<tab>STATE
        // Example: 60	alsa_output.pci-0000_00_1f.3.iec958-stereo.monitor	PipeWire	s32le 2ch 48000Hz	SUSPENDED
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            let source_name = parts[1].trim();
            if source_name.ends_with(".monitor") {
                info!("[Linux Audio] Found PulseAudio monitor: {}", source_name);
                monitors.push((source_name.to_string(), source_name.to_string()));
            }
        }
    }

    monitors
}

/// Get PulseAudio/PipeWire input sources (microphones) via pactl command
fn get_pulseaudio_inputs() -> Vec<String> {
    let mut inputs = Vec::new();

    let output = match Command::new("pactl")
        .args(["list", "sources", "short"])
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            debug!("[Linux Audio] Failed to run pactl for inputs: {}", e);
            return inputs;
        }
    };

    if !output.status.success() {
        return inputs;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            let source_name = parts[1].trim();
            // Include non-monitor sources as potential microphones
            if !source_name.ends_with(".monitor") {
                debug!("[Linux Audio] Found PulseAudio input: {}", source_name);
                inputs.push(source_name.to_string());
            }
        }
    }

    inputs
}

/// Configure Linux audio devices using PulseAudio/PipeWire and ALSA
///
/// On Linux with PulseAudio/PipeWire, system audio capture works via monitor sources.
/// Monitor sources have names ending in ".monitor" and capture audio playing through
/// the corresponding output sink.
///
/// This function:
/// 1. Gets PulseAudio/PipeWire monitors via `pactl` (preferred for system audio)
/// 2. Gets PulseAudio/PipeWire inputs via `pactl` (for microphones)
/// 3. Falls back to ALSA devices from cpal if pactl unavailable
/// 4. Marks monitors as DeviceType::Output for UI display in "System Audio" dropdown
pub fn configure_linux_audio(host: &cpal::Host) -> Result<Vec<AudioDevice>> {
    let mut devices = Vec::new();
    let card_names = get_card_friendly_names();

    info!("[Linux Audio] Card friendly names: {:?}", card_names);

    // First, try to get PulseAudio/PipeWire monitors (best for system audio)
    info!("[Linux Audio] Checking for PulseAudio/PipeWire monitors...");
    let pa_monitors = get_pulseaudio_monitors();
    let pa_inputs = get_pulseaudio_inputs();

    if !pa_monitors.is_empty() {
        info!("[Linux Audio] Found {} PulseAudio/PipeWire monitor sources", pa_monitors.len());

        // Add PulseAudio monitors as system audio devices
        for (name, _desc) in &pa_monitors {
            let friendly = make_monitor_friendly_name(name, &card_names);
            info!("[Linux Audio] Adding monitor for system audio: {} -> {}", name, friendly);
            devices.push(AudioDevice::new(name.clone(), DeviceType::Output));
        }
    }

    if !pa_inputs.is_empty() {
        info!("[Linux Audio] Found {} PulseAudio/PipeWire input sources", pa_inputs.len());

        // Add PulseAudio inputs as microphones
        for name in &pa_inputs {
            let friendly = make_friendly_name(name, &card_names);
            info!("[Linux Audio] Adding PulseAudio input (microphone): {} -> {}", name, friendly);
            devices.push(AudioDevice::new(name.clone(), DeviceType::Input));
        }
    }

    // Also enumerate ALSA devices from cpal for fallback/additional devices
    info!("[Linux Audio] Enumerating ALSA devices from cpal...");
    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            // Skip if we already have this device from PulseAudio
            if devices.iter().any(|d| d.name == name) {
                continue;
            }

            debug!("[Linux Audio] Found ALSA input device: {}", name);

            if name.contains(".monitor") || name.to_lowercase().contains("monitor") {
                // Monitor source for system audio
                let friendly = make_monitor_friendly_name(&name, &card_names);
                info!("[Linux Audio] Found ALSA monitor (system audio): {} -> {}", name, friendly);
                devices.push(AudioDevice::new(name, DeviceType::Output));
            } else if name.contains("Loopback") || name.to_lowercase().contains("loopback") {
                // ALSA loopback device for system audio
                let friendly = make_friendly_name(&name, &card_names);
                info!("[Linux Audio] Found ALSA loopback (system audio): {} -> {}", name, friendly);
                devices.push(AudioDevice::new(name, DeviceType::Output));
            } else {
                // Regular microphone
                let friendly = make_friendly_name(&name, &card_names);
                info!("[Linux Audio] Found ALSA microphone: {} -> {}", name, friendly);
                devices.push(AudioDevice::new(name, DeviceType::Input));
            }
        }
    }

    // Count device types for logging
    let mic_count = devices.iter().filter(|d| d.device_type == DeviceType::Input).count();
    let sys_count = devices.iter().filter(|d| d.device_type == DeviceType::Output).count();

    info!("[Linux Audio] Total devices found: {} ({} microphones, {} system audio sources)",
          devices.len(), mic_count, sys_count);

    if sys_count == 0 {
        warn!("[Linux Audio] No monitor sources found! System audio capture may not work.");
        warn!("[Linux Audio] Ensure PulseAudio/PipeWire is running.");
        warn!("[Linux Audio] Run 'pactl list sources short' to check available sources.");
    }

    Ok(devices)
}

/// Create a user-friendly name for monitor sources
/// Extracts meaningful info from PulseAudio monitor names like:
/// "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor" -> "Built-in Audio (System Audio)"
fn make_monitor_friendly_name(monitor_name: &str, card_names: &HashMap<String, String>) -> String {
    // Try to extract card identifier from the monitor name
    // Format: alsa_output.{card_info}.{profile}.monitor

    // Check for common patterns
    if monitor_name.contains("pci-") {
        // Built-in audio card
        return "Built-in Audio (System Audio)".to_string();
    }

    if monitor_name.contains("usb-") {
        // USB audio device
        // Try to find friendly name from card_names
        for (card_id, friendly) in card_names {
            if monitor_name.to_lowercase().contains(&card_id.to_lowercase()) {
                return format!("{} (System Audio)", friendly);
            }
        }
        return "USB Audio (System Audio)".to_string();
    }

    if monitor_name.contains("hdmi") || monitor_name.contains("HDMI") {
        return "HDMI Audio (System Audio)".to_string();
    }

    if monitor_name.contains("Loopback") || monitor_name.contains("loopback") {
        return "System Loopback (System Audio)".to_string();
    }

    if monitor_name.contains("bluetooth") || monitor_name.contains("bluez") {
        return "Bluetooth (System Audio)".to_string();
    }

    // Default: just add system audio suffix
    format!("{} (System Audio)", monitor_name)
}