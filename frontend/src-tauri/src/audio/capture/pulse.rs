// PulseAudio/PipeWire system-audio (loopback) capture for Linux.
//
// cpal's ALSA backend cannot enumerate or open PulseAudio/PipeWire "monitor"
// sources: ALSA's `snd_device_name_hint` only reports genuine ALSA PCM
// devices, and PulseAudio/PipeWire never register per-sink monitors as ALSA
// hardware devices (confirmed: `arecord -L` never lists them). WASAPI
// (Windows) and Core Audio (macOS) both support native loopback capture
// through cpal/cidre; ALSA has no equivalent, so system audio devices
// selected via cpal on Linux (mistakenly labelled as monitors, or as the
// raw default *playback* device) always fail to open as an input stream and
// recording silently falls back to microphone-only.
//
// Monitor sources are only visible through PulseAudio's own client
// protocol. Rather than linking libpulse directly (the `-devel`/`-dev`
// headers are frequently missing on end-user systems), this module shells
// out to `pactl`/`parec`, which ship in `pulseaudio-utils` and are also
// provided by PipeWire's `pipewire-pulse` compatibility layer -- i.e.
// present on essentially every modern Linux desktop.

use anyhow::{anyhow, Result};
use std::process::{Child, Command, Stdio};

/// A PulseAudio/PipeWire source that monitors a sink's output, i.e. usable
/// for system-audio/loopback capture.
#[derive(Debug, Clone)]
pub struct PulseMonitorSource {
    /// Technical PulseAudio source name, e.g. "alsa_output.pci-....monitor"
    pub name: String,
    /// Human-readable description, e.g. "Monitor of Built-in Audio Speaker"
    pub description: String,
    /// Name of the sink this source monitors.
    pub monitor_of_sink: String,
}

/// List every PulseAudio/PipeWire source that monitors a sink (i.e. every
/// source usable for system-audio capture). Requires `pactl`.
pub fn list_monitor_sources() -> Result<Vec<PulseMonitorSource>> {
    let output = Command::new("pactl")
        .args(["list", "sources"])
        .output()
        .map_err(|e| {
            anyhow!(
                "Failed to run `pactl` ({e}); install `pulseaudio-utils` (or `pipewire-pulse`) for system audio capture"
            )
        })?;

    if !output.status.success() {
        return Err(anyhow!(
            "`pactl list sources` exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(parse_source_list(&String::from_utf8_lossy(&output.stdout)))
}

/// Parse the plain-text output of `pactl list sources` into monitor-only
/// entries (sources whose `Monitor of Sink:` field is not `n/a`).
fn parse_source_list(text: &str) -> Vec<PulseMonitorSource> {
    let mut sources = Vec::new();

    let mut name: Option<String> = None;
    let mut description: Option<String> = None;
    let mut monitor_of_sink: Option<String> = None;

    fn take(
        name: &mut Option<String>,
        description: &mut Option<String>,
        monitor_of_sink: &mut Option<String>,
    ) -> Option<PulseMonitorSource> {
        let name = name.take()?;
        let description = description.take()?;
        let monitor_of_sink = monitor_of_sink.take()?;
        if monitor_of_sink == "n/a" {
            return None;
        }
        Some(PulseMonitorSource { name, description, monitor_of_sink })
    }

    for line in text.lines() {
        if line.starts_with("Source #") {
            if let Some(source) = take(&mut name, &mut description, &mut monitor_of_sink) {
                sources.push(source);
            }
            continue;
        }

        let trimmed = line.trim_start();
        if let Some(v) = trimmed.strip_prefix("Name: ") {
            name = Some(v.trim().to_string());
        } else if let Some(v) = trimmed.strip_prefix("Description: ") {
            description = Some(v.trim().to_string());
        } else if let Some(v) = trimmed.strip_prefix("Monitor of Sink: ") {
            monitor_of_sink = Some(v.trim().to_string());
        }
    }
    if let Some(source) = take(&mut name, &mut description, &mut monitor_of_sink) {
        sources.push(source);
    }

    sources
}

/// Resolve a display name (as produced by `list_monitor_sources` and shown
/// in the device picker) back to the technical PulseAudio source name.
pub fn resolve_monitor_source_name(display_name: &str) -> Result<String> {
    let sources = list_monitor_sources()?;

    sources
        .iter()
        .find(|s| s.description == display_name)
        .or_else(|| sources.iter().find(|s| s.name == display_name))
        .map(|s| s.name.clone())
        .ok_or_else(|| {
            anyhow!(
                "No PulseAudio/PipeWire monitor source matching '{}' (available: {})",
                display_name,
                sources.iter().map(|s| s.description.as_str()).collect::<Vec<_>>().join(", ")
            )
        })
}

/// Get the monitor source for the current default playback sink, if any.
pub fn default_monitor_source() -> Result<PulseMonitorSource> {
    let sink = default_sink_name()?;
    let sources = list_monitor_sources()?;
    sources
        .into_iter()
        .find(|s| s.monitor_of_sink == sink)
        .ok_or_else(|| anyhow!("No monitor source found for default sink '{}'", sink))
}

fn default_sink_name() -> Result<String> {
    let output = Command::new("pactl")
        .arg("get-default-sink")
        .output()
        .map_err(|e| anyhow!("Failed to run `pactl get-default-sink`: {e}"))?;

    if !output.status.success() {
        return Err(anyhow!(
            "`pactl get-default-sink` exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() {
        return Err(anyhow!("`pactl get-default-sink` returned an empty sink name"));
    }
    Ok(name)
}

/// Spawn a `parec` process streaming raw interleaved little-endian f32 PCM
/// captured from the given monitor source to stdout.
pub fn spawn_monitor_capture(source_name: &str, sample_rate: u32, channels: u16) -> Result<Child> {
    let mut child = Command::new("parec")
        .arg("--raw")
        .arg(format!("--device={}", source_name))
        .arg(format!("--rate={}", sample_rate))
        .arg(format!("--channels={}", channels))
        .arg("--format=float32le")
        .arg("--client-name=meetily-system-audio")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            anyhow!(
                "Failed to spawn `parec` for system audio capture (is `pulseaudio-utils`/`pipewire-pulse` installed?): {e}"
            )
        })?;

    // Drain and log stderr on a background thread: without this, parec can
    // block writing to a full pipe once its stderr buffer fills, and real
    // failures (e.g. an invalid source name) would otherwise vanish silently.
    if let Some(stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            for line in BufReader::new(stderr).lines().flatten() {
                log::warn!("parec: {}", line);
            }
        });
    }

    Ok(child)
}

/// Whether the PulseAudio/PipeWire CLI tools needed for system audio capture
/// are available on this system.
pub fn is_available() -> bool {
    Command::new("pactl")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_monitor_and_skips_plain_sources() {
        let sample = "\
Source #53
\tState: SUSPENDED
\tName: alsa_input.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__Mic2__source
\tDescription: Meteor Lake-P HD Audio Controller Stereo Microphone
\tMonitor of Sink: n/a

Source #2520
\tState: RUNNING
\tName: alsa_output.usb-Apple__Inc._EarPods_N9FYLVQL6F-00.analog-stereo.monitor
\tDescription: Monitor of EarPods Analog Stereo
\tMonitor of Sink: alsa_output.usb-Apple__Inc._EarPods_N9FYLVQL6F-00.analog-stereo
";

        let sources = parse_source_list(sample);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "alsa_output.usb-Apple__Inc._EarPods_N9FYLVQL6F-00.analog-stereo.monitor");
        assert_eq!(sources[0].description, "Monitor of EarPods Analog Stereo");
        assert_eq!(sources[0].monitor_of_sink, "alsa_output.usb-Apple__Inc._EarPods_N9FYLVQL6F-00.analog-stereo");
    }
}
