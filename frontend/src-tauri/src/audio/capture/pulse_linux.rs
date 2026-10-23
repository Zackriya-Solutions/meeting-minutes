// Native PipeWire/PulseAudio system-audio capture for Linux.
//
// Bypasses cpal's ALSA-only host entirely by talking to the PulseAudio client
// protocol directly (which PipeWire also implements via pipewire-pulse). This
// avoids the need to hand-register monitor sources as named ALSA pseudo-devices
// in ~/.asoundrc: sink descriptions and monitor sources come straight from the
// server's real metadata.
//
// Device enumeration (list_sinks, list_sources, default_source_name)
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
    /// Exact PulseAudio sink name: the identity of the "System Audio" device.
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

/// Resolve a sink by its exact PulseAudio name. A missing sink is an error:
/// there is deliberately no fallback to another endpoint.
pub fn find_sink_by_name(name: &str) -> Result<PulseSink> {
    pick_sink(list_sinks()?, name)
}

fn pick_sink(sinks: Vec<PulseSink>, name: &str) -> Result<PulseSink> {
    sinks
        .into_iter()
        .find(|sink| sink.name == name)
        .ok_or_else(|| not_found_error(name))
}

fn not_found_error(name: &str) -> anyhow::Error {
    anyhow!("Périphérique '{}' introuvable, re-sélectionnez-le", name)
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

/// Resolve a source by its exact PulseAudio name. A missing source is an
/// error: there is deliberately no fallback to another endpoint.
pub fn find_source_by_name(name: &str) -> Result<PulseSource> {
    pick_source(list_sources()?, name)
}

fn pick_source(sources: Vec<PulseSource>, name: &str) -> Result<PulseSource> {
    sources
        .into_iter()
        .find(|source| source.source_name == name)
        .ok_or_else(|| not_found_error(name))
}

/// Exact name of the server's default input source, if any.
pub fn default_source_name() -> Result<Option<String>> {
    pulse_enumerator::default_source_name()
}

/// Exact name of the server's default sink, if any.
pub fn default_sink_name() -> Result<Option<String>> {
    pulse_enumerator::default_sink_name()
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

    fn sink(name: &str, description: &str) -> PulseSink {
        PulseSink {
            name: name.to_string(),
            description: description.to_string(),
            monitor_source_name: format!("{}.monitor", name),
        }
    }

    fn source(name: &str, description: &str) -> PulseSource {
        PulseSource {
            description: description.to_string(),
            source_name: name.to_string(),
        }
    }

    #[test]
    fn pick_sink_matches_exact_name_among_same_description() {
        let sinks = vec![sink("sink_a", "Speakers"), sink("sink_b", "Speakers")];
        let picked = pick_sink(sinks, "sink_b").expect("sink_b exists");
        assert_eq!(picked.monitor_source_name, "sink_b.monitor");
    }

    #[test]
    fn pick_sink_missing_is_reselect_error() {
        let err = pick_sink(vec![sink("sink_a", "Speakers")], "gone").unwrap_err();
        assert!(err.to_string().contains("re-sélectionnez-le"));
        assert!(err.to_string().contains("gone"));
    }

    #[test]
    fn pick_sink_does_not_match_on_description() {
        assert!(pick_sink(vec![sink("sink_a", "Speakers")], "Speakers").is_err());
    }

    #[test]
    fn pick_source_matches_exact_name_among_same_description() {
        let sources = vec![source("src_a", "Mic"), source("src_b", "Mic")];
        assert_eq!(pick_source(sources, "src_b").unwrap().source_name, "src_b");
    }

    #[test]
    fn pick_source_missing_is_reselect_error() {
        let err = pick_source(vec![], "src_x").unwrap_err();
        assert!(err.to_string().contains("re-sélectionnez-le"));
    }
}
