// Native PipeWire/PulseAudio system-audio capture for Linux.
//
// Bypasses cpal's ALSA-only host entirely by talking to the PulseAudio client
// protocol directly (which PipeWire also implements via pipewire-pulse). This
// avoids the need to hand-register monitor sources as named ALSA pseudo-devices
// in ~/.asoundrc: sink descriptions and monitor sources come straight from the
// server's real metadata.
//
// Device enumeration (list_sinks, list_sources, default_source_description)
// is delegated to `pulse_enumerator.rs`, which keeps a single persistent
// libpulse connection on a dedicated thread instead of opening and destroying
// two fresh connections every 2 seconds for the whole meeting. See TECH-02
// for the rationale.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use libpulse_binding::sample::{Format, Spec};
use libpulse_binding::stream::Direction;
use libpulse_simple_binding::Simple;
use log::{debug, warn};

use super::pulse_enumerator;

/// A PulseAudio sink, with the metadata needed to offer it as a "System Audio"
/// capture device: a real display description and its monitor source name.
#[derive(Debug, Clone)]
pub struct PulseSink {
    /// Real PulseAudio sink name, as reported by the server in `default_sink_name`.
    pub name: String,
    pub description: String,
    pub monitor_source_name: String,
}

/// A PulseAudio input source (microphone, line-in, …), excluding sink monitors.
#[derive(Debug, Clone)]
pub struct PulseSource {
    /// Human-readable description straight from the server — this is the exact
    /// string KDE's own audio settings show (e.g. "Ryzen HD Audio Controller
    /// Headset Mono Microphone").
    pub description: String,
    /// Real PulseAudio source name, e.g.
    /// "alsa_input.pci-0000_c1_00.6.HiFi__Headset__source".
    pub source_name: String,
}

/// Fixed capture format requested from the server. PulseAudio/PipeWire
/// transparently resamples and remixes the monitor source to this spec
/// server-side, so the rest of the pipeline (which expects 48kHz) never needs
/// to know the sink's native sample rate or channel count.
const CAPTURE_SAMPLE_RATE: u32 = 48000;

/// List all sinks (playback outputs) with their monitor source name, for use as
/// "System Audio" capture devices. Descriptions come straight from the server
/// and are refreshed on every call: a persistent connection does not cache
/// server state, it only avoids the repeated connect/handshake/disconnect churn
/// (see TECH-02).
pub fn list_sinks() -> Result<Vec<PulseSink>> {
    debug!("pulse_linux::list_sinks: requesting");
    let result = pulse_enumerator::list_sinks();
    if let Ok(ref sinks) = result {
        debug!("pulse_linux::list_sinks: got {} sink(s)", sinks.len());
    }
    result
}

/// The sink the server currently routes playback to (`default_sink_name`), or
/// `None` if the server reports no default or the sink has no monitor source.
pub fn default_sink() -> Result<Option<PulseSink>> {
    let Some(default_name) = pulse_enumerator::default_sink_name()? else {
        return Ok(None);
    };
    let sink = list_sinks()?.into_iter().find(|s| s.name == default_name);
    if sink.is_none() {
        warn!(
            "pulse_linux::default_sink: default sink '{}' not found in sink list",
            default_name
        );
    }
    Ok(sink)
}

/// Resolve a sink's real monitor source name from its display description (as
/// shown in the "System Audio" picker, e.g. "JBL Tune 770NC").
pub fn find_monitor_source_by_description(description: &str) -> Result<String> {
    list_sinks()?
        .into_iter()
        .find(|sink| sink.description == description)
        .map(|sink| sink.monitor_source_name)
        .ok_or_else(|| anyhow!("No PulseAudio sink found matching '{}'", description))
}

/// List all real input sources (sink monitors excluded — those are offered as
/// "System Audio" devices via `list_sinks`).
pub fn list_sources() -> Result<Vec<PulseSource>> {
    debug!("pulse_linux::list_sources: requesting");
    let result = pulse_enumerator::list_sources();
    if let Ok(ref sources) = result {
        debug!("pulse_linux::list_sources: got {} source(s)", sources.len());
    }
    result
}

/// Resolve a source's real PulseAudio name from its display description.
pub fn find_source_by_description(description: &str) -> Result<String> {
    let mut matches = list_sources()?
        .into_iter()
        .filter(|source| source.description == description)
        .peekable();

    if matches.peek().is_some() {
        let count = matches.clone().count();
        if count > 1 {
            warn!(
                "pulse_linux::find_source_by_description: {} sources share the description '{}'; using the first one",
                count, description
            );
        }
    }

    matches
        .next()
        .map(|source| source.source_name)
        .ok_or_else(|| anyhow!("No PulseAudio source found matching '{}'", description))
}

/// Description of the server's default input source, if any.
/// Used to resolve "Default Microphone".
pub fn default_source_description() -> Result<Option<String>> {
    debug!("pulse_linux::default_source_description: requesting server info");
    let default_name = pulse_enumerator::default_source_name()?;

    let Some(default_name) = default_name else {
        return Ok(None);
    };

    // Resolve the display description for the default source. The second
    // introspection call now travels through the same persistent connection as
    // the first one, so this no longer opens a second libpulse client.
    let sources = list_sources()?;
    let default_source = sources.iter().find(|s| s.source_name == default_name);

    if let Some(source) = default_source {
        debug!("pulse_linux::default_source_description: resolved");
        Ok(Some(source.description.clone()))
    } else {
        warn!(
            "pulse_linux::default_source_description: default source '{}' not found in source list",
            default_name
        );
        Ok(None)
    }
}

/// Blocking PulseAudio record stream, backed by libpulse-simple's record API.
/// Used both for sink monitors (system audio) and real input sources
/// (microphones).
pub struct PulseCapture {
    simple: Simple,
    should_stop: Arc<AtomicBool>,
    channels: u16,
}

impl PulseCapture {
    fn new_with_channels(source_name: &str, channels: u16, stream_label: &str) -> Result<Self> {
        let spec = Spec {
            format: Format::FLOAT32NE,
            channels: channels as u8,
            rate: CAPTURE_SAMPLE_RATE,
        };
        if !spec.is_valid() {
            return Err(anyhow!("Invalid PulseAudio sample spec"));
        }

        let simple = Simple::new(
            None, // default server
            "Meetily",
            Direction::Record,
            Some(source_name),
            stream_label,
            &spec,
            None, // default channel map
            None, // default buffering attributes
        )
        .map_err(|e| {
            anyhow!(
                "Failed to open PulseAudio record stream on '{}': {}",
                source_name, e
            )
        })?;

        Ok(Self {
            simple,
            should_stop: Arc::new(AtomicBool::new(false)),
            channels,
        })
    }

    /// System audio: stereo capture of a sink's monitor source.
    pub fn new_system(monitor_source_name: &str) -> Result<Self> {
        Self::new_with_channels(monitor_source_name, 2, "System Audio")
    }

    /// Microphone: mono capture of a real input source. PulseAudio downmixes and
    /// resamples server-side, so the pipeline still receives 48kHz.
    pub fn new_microphone(source_name: &str) -> Result<Self> {
        Self::new_with_channels(source_name, 1, "Microphone")
    }

    pub fn sample_rate(&self) -> u32 {
        CAPTURE_SAMPLE_RATE
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Handle used to signal the capture loop (running on another thread) to stop.
    pub fn stop_handle(&self) -> Arc<AtomicBool> {
        self.should_stop.clone()
    }

    /// Runs a blocking read loop, invoking `on_samples` with interleaved f32
    /// frames as they arrive. Meant to run on a `spawn_blocking` task, since
    /// PulseAudio's simple API blocks on `read()`. Returns once `stop_handle()`
    /// is signalled or the stream errors out.
    pub fn run(&self, mut on_samples: impl FnMut(&[f32])) {
        // ~21ms at 48kHz stereo: matches the 1024-frame chunking used by the
        // macOS Core Audio path.
        const FRAMES_PER_CHUNK: usize = 1024;
        let mut byte_buf = vec![0u8; FRAMES_PER_CHUNK * self.channels as usize * 4];
        let mut sample_buf = Vec::with_capacity(FRAMES_PER_CHUNK * self.channels as usize);

        while !self.should_stop.load(Ordering::Acquire) {
            if let Err(e) = self.simple.read(&mut byte_buf) {
                warn!("PulseAudio record stream read error: {}", e);
                break;
            }

            // Re-check right after the blocking read returns: if stop() timed
            // out waiting for this thread (stalled source) and moved on, a
            // late read() shouldn't inject one more chunk into whatever
            // pipeline state now exists (new stream, new recording).
            if self.should_stop.load(Ordering::Acquire) {
                break;
            }

            sample_buf.clear();
            sample_buf.extend(
                byte_buf
                    .chunks_exact(4)
                    .map(|b| f32::from_ne_bytes([b[0], b[1], b[2], b[3]])),
            );

            on_samples(&sample_buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires a running PulseAudio/PipeWire server; run manually.
    fn test_list_sinks() {
        let sinks = list_sinks().expect("Failed to list sinks");
        for sink in &sinks {
            println!("sink: {} -> monitor {}", sink.description, sink.monitor_source_name);
        }
        assert!(!sinks.is_empty(), "Expected at least one sink on a machine with audio output");
    }

    #[test]
    #[ignore] // Requires a running PulseAudio/PipeWire server; run manually.
    fn test_list_sources() {
        let sources = list_sources().expect("Failed to list sources");
        for source in &sources {
            println!("source: {} -> {}", source.description, source.source_name);
        }
        assert!(!sources.is_empty(), "Expected at least one real input source on a machine with audio input");
    }

    #[test]
    #[ignore] // Requires real audio playback during the test; run manually.
    fn test_capture_reads_nonzero_samples() {
        let sinks = list_sinks().expect("Failed to list sinks");
        let sink = sinks
            .into_iter()
            .find(|s| s.monitor_source_name.to_lowercase().contains("bluez")
                || s.monitor_source_name.to_lowercase().contains("running"))
            .or_else(|| list_sinks().unwrap().into_iter().next())
            .expect("No sink available");

        println!("Capturing from: {} ({})", sink.description, sink.monitor_source_name);
        let capture = PulseCapture::new_system(&sink.monitor_source_name).expect("Failed to open capture");
        let stop = capture.stop_handle();

        let received = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let received_clone = received.clone();
        let stop_clone = stop.clone();
        let handle = std::thread::spawn(move || {
            capture.run(|samples| {
                received_clone.fetch_add(samples.len(), std::sync::atomic::Ordering::Relaxed);
                if received_clone.load(std::sync::atomic::Ordering::Relaxed) > 48000 * 2 {
                    stop_clone.store(true, std::sync::atomic::Ordering::Release);
                }
            });
        });

        std::thread::sleep(std::time::Duration::from_secs(3));
        stop.store(true, std::sync::atomic::Ordering::Release);
        handle.join().unwrap();

        let total = received.load(std::sync::atomic::Ordering::Relaxed);
        println!("Received {} samples", total);
        assert!(total > 0, "Expected to receive some samples from the monitor source");
    }
}
