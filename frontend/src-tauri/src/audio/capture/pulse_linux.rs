// Native PipeWire/PulseAudio system-audio capture for Linux.
//
// Bypasses cpal's ALSA-only host entirely by talking to the PulseAudio client
// protocol directly (which PipeWire also implements via pipewire-pulse). This
// avoids the need to hand-register monitor sources as named ALSA pseudo-devices
// in ~/.asoundrc: sink descriptions and monitor sources come straight from the
// server's real metadata.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::introspect::ServerInfo;
use libpulse_binding::context::{Context, FlagSet as ContextFlagSet, State as ContextState};
use libpulse_binding::mainloop::standard::{IterateResult, Mainloop};
use libpulse_binding::operation::{Operation, State as OperationState};
use libpulse_binding::proplist::Proplist;
use libpulse_binding::sample::{Format, Spec};
use libpulse_binding::stream::Direction;
use libpulse_simple_binding::Simple;
use log::{info, warn};

/// A PulseAudio sink, with the metadata needed to offer it as a "System Audio"
/// capture device: a real display description and its monitor source name.
#[derive(Debug, Clone)]
pub struct PulseSink {
    pub description: String,
    pub monitor_source_name: String,
}

/// Fixed capture format requested from the server. PulseAudio/PipeWire
/// transparently resamples and remixes the monitor source to this spec
/// server-side, so the rest of the pipeline (which expects 48kHz) never needs
/// to know the sink's native sample rate or channel count.
const CAPTURE_SAMPLE_RATE: u32 = 48000;

/// Pump the mainloop until `operation` finishes, blocking between iterations.
fn run_operation_to_completion<T: ?Sized>(
    mainloop: &mut Mainloop,
    operation: &Operation<T>,
) -> Result<()> {
    loop {
        match mainloop.iterate(true) {
            IterateResult::Quit(_) | IterateResult::Err(_) => {
                return Err(anyhow!("PulseAudio mainloop iteration failed"));
            }
            IterateResult::Success(_) => {}
        }

        match operation.get_state() {
            OperationState::Done => return Ok(()),
            OperationState::Cancelled => {
                return Err(anyhow!("PulseAudio operation was cancelled"));
            }
            OperationState::Running => continue,
        }
    }
}

/// Connect a fresh context to the default PulseAudio/PipeWire server, blocking
/// until it's ready. Each call opens (and the caller later drops) its own
/// connection — unlike ALSA's cached global config, there's no stale-state
/// problem to work around here, so every call sees the server's current sinks.
fn connect() -> Result<(Mainloop, Context)> {
    info!("🔊 pulse_linux::connect: creating mainloop");
    let mut mainloop =
        Mainloop::new().ok_or_else(|| anyhow!("Failed to create PulseAudio mainloop"))?;

    let proplist =
        Proplist::new().ok_or_else(|| anyhow!("Failed to create PulseAudio proplist"))?;
    let mut context = Context::new_with_proplist(&mainloop, "Meetily", &proplist)
        .ok_or_else(|| anyhow!("Failed to create PulseAudio context"))?;

    info!("🔊 pulse_linux::connect: calling context.connect()");
    context
        .connect(None, ContextFlagSet::NOFLAGS, None)
        .map_err(|e| anyhow!("Failed to connect to PulseAudio/PipeWire server: {}", e))?;

    info!("🔊 pulse_linux::connect: waiting for context to become Ready");
    let mut iterations: u32 = 0;
    loop {
        iterations += 1;
        if iterations % 50 == 0 {
            info!("🔊 pulse_linux::connect: still waiting after {} iterations, state={:?}", iterations, context.get_state());
        }

        match mainloop.iterate(true) {
            IterateResult::Quit(_) | IterateResult::Err(_) => {
                return Err(anyhow!(
                    "PulseAudio mainloop iteration failed while connecting"
                ));
            }
            IterateResult::Success(_) => {}
        }

        match context.get_state() {
            ContextState::Ready => break,
            ContextState::Failed | ContextState::Terminated => {
                return Err(anyhow!(
                    "PulseAudio/PipeWire context connection failed or was terminated"
                ));
            }
            _ => {}
        }
    }
    info!("🔊 pulse_linux::connect: context Ready after {} iterations", iterations);

    Ok((mainloop, context))
}

/// List all sinks (playback outputs) with their monitor source name, for use as
/// "System Audio" capture devices. Descriptions come straight from the server —
/// no manual ~/.asoundrc registration needed, and switching outputs (new DAC,
/// different Bluetooth device) is picked up on the next call since nothing here
/// is cached between calls.
pub fn list_sinks() -> Result<Vec<PulseSink>> {
    info!("🔊 pulse_linux::list_sinks: connecting");
    let (mut mainloop, context) = connect()?;

    info!("🔊 pulse_linux::list_sinks: connected, requesting sink list");
    let sinks: Rc<RefCell<Vec<PulseSink>>> = Rc::new(RefCell::new(Vec::new()));
    let sinks_cb = sinks.clone();

    let operation = context.introspect().get_sink_info_list(move |result| {
        if let ListResult::Item(info) = result {
            let monitor_source_name = info.monitor_source_name.as_deref().unwrap_or_default();
            if monitor_source_name.is_empty() {
                return;
            }

            let description = info
                .description
                .as_deref()
                .unwrap_or("Unknown output")
                .to_string();

            sinks_cb.borrow_mut().push(PulseSink {
                description,
                monitor_source_name: monitor_source_name.to_string(),
            });
        }
    });

    info!("🔊 pulse_linux::list_sinks: pumping mainloop until sink list operation completes");
    run_operation_to_completion(&mut mainloop, &operation)?;
    drop(operation);

    let result = sinks.borrow().clone();
    info!("🔊 pulse_linux::list_sinks: got {} sink(s)", result.len());
    Ok(result)
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

/// List all real input sources (sink monitors excluded — those are offered as
/// "System Audio" devices via `list_sinks`).
pub fn list_sources() -> Result<Vec<PulseSource>> {
    info!("🎤 pulse_linux::list_sources: connecting");
    let (mut mainloop, context) = connect()?;

    info!("🎤 pulse_linux::list_sources: connected, requesting source list");
    let sources: Rc<RefCell<Vec<PulseSource>>> = Rc::new(RefCell::new(Vec::new()));
    let sources_cb = sources.clone();

    let operation = context.introspect().get_source_info_list(move |result| {
        if let ListResult::Item(info) = result {
            // Monitors of sinks are already exposed as "System Audio" devices
            // through list_sinks(); ignore them here.
            if info.monitor_of_sink.is_some() {
                return;
            }

            let source_name = info.name.as_deref().unwrap_or_default();
            if source_name.is_empty() {
                return;
            }

            let description = info
                .description
                .as_deref()
                .unwrap_or(source_name)
                .to_string();

            sources_cb.borrow_mut().push(PulseSource {
                description,
                source_name: source_name.to_string(),
            });
        }
    });

    info!("🎤 pulse_linux::list_sources: pumping mainloop until source list operation completes");
    run_operation_to_completion(&mut mainloop, &operation)?;
    drop(operation);

    let result = sources.borrow().clone();
    info!("🎤 pulse_linux::list_sources: got {} source(s)", result.len());
    Ok(result)
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
                "🎤 pulse_linux::find_source_by_description: {} sources share the description '{}'; using the first one",
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
    info!("🎤 pulse_linux::default_source_description: connecting");
    let (mut mainloop, context) = connect()?;

    info!("🎤 pulse_linux::default_source_description: requesting server info");
    let default_name: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let default_name_cb = default_name.clone();

    let operation = context.introspect().get_server_info(move |info: &ServerInfo| {
        if let Some(name) = info.default_source_name.as_deref() {
            *default_name_cb.borrow_mut() = Some(name.to_string());
        }
    });

    run_operation_to_completion(&mut mainloop, &operation)?;
    drop(operation);

    let default_name = default_name.borrow().clone();
    let Some(default_name) = default_name else {
        return Ok(None);
    };

    info!(
        "🎤 pulse_linux::default_source_description: default source name is '{}'",
        default_name
    );

    // Resolve the display description for the default source. Do a second
    // introspection call: it's rare (startup of a recording) and keeps the code
    // straightforward.
    let sources = list_sources()?;
    let default_source = sources.iter().find(|s| s.source_name == default_name);

    if let Some(source) = default_source {
        info!(
            "🎤 pulse_linux::default_source_description: default source description is '{}'",
            source.description
        );
        Ok(Some(source.description.clone()))
    } else {
        warn!(
            "🎤 pulse_linux::default_source_description: default source '{}' not found in source list",
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
                source_name,
                e
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
