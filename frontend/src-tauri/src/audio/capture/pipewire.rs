// Linux system audio capture through PipeWire.
//
// ALSA (what cpal uses on Linux) does not expose PulseAudio/PipeWire monitor sources,
// so system audio is captured by running `pw-record` against the default sink's
// monitor and reading raw f32 PCM from its stdout.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::thread::JoinHandle;

use anyhow::{anyhow, Result};
use log::{error, info, warn};

use crate::audio::pipeline::AudioCapture;

/// Device name shown to users for PipeWire system audio capture
pub const PIPEWIRE_SYSTEM_DEVICE: &str = "System Audio (PipeWire)";

/// Sample rate requested from PipeWire (matches the pipeline's 48kHz target)
pub const PIPEWIRE_SAMPLE_RATE: u32 = 48_000;

/// Samples handed to the pipeline per read (~21ms at 48kHz)
const SAMPLES_PER_READ: usize = 1024;

/// Whether `pw-record` is installed and runnable
pub fn is_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("pw-record")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    })
}

/// A running `pw-record` process feeding the default sink monitor into the pipeline
pub struct PipeWireSystemCapture {
    child: Child,
    reader: Option<JoinHandle<()>>,
}

impl PipeWireSystemCapture {
    pub fn start(capture: AudioCapture) -> Result<Self> {
        let mut child = Command::new("pw-record")
            .args([
                "--raw",
                "--format",
                "f32",
                "--rate",
                &PIPEWIRE_SAMPLE_RATE.to_string(),
                "--channels",
                "1",
                "-P",
                "{ stream.capture.sink=true node.name=meetily-system-audio }",
                "-",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| anyhow!("Failed to start pw-record: {}", e))?;

        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("pw-record stdout was not captured"))?;

        let reader = std::thread::Builder::new()
            .name("pipewire-system-audio".to_string())
            .spawn(move || {
                let mut bytes = vec![0u8; SAMPLES_PER_READ * 4];
                let mut pending = Vec::with_capacity(SAMPLES_PER_READ * 4 + 3);
                let mut samples = Vec::with_capacity(SAMPLES_PER_READ);

                loop {
                    match stdout.read(&mut bytes) {
                        Ok(0) => break,
                        Ok(n) => {
                            pending.extend_from_slice(&bytes[..n]);
                            let whole = pending.len() - pending.len() % 4;
                            samples.clear();
                            samples.extend(
                                pending[..whole]
                                    .chunks_exact(4)
                                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                            );
                            pending.drain(..whole);
                            if !samples.is_empty() {
                                capture.process_audio_data(&samples);
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(e) => {
                            error!("Failed to read PipeWire system audio: {}", e);
                            break;
                        }
                    }
                }
                info!("PipeWire system audio reader stopped");
            })
            .map_err(|e| anyhow!("Failed to spawn PipeWire reader thread: {}", e))?;

        info!("PipeWire system audio capture started (default sink monitor)");
        Ok(Self {
            child,
            reader: Some(reader),
        })
    }

    pub fn stop(self) {
        drop(self);
    }
}

impl Drop for PipeWireSystemCapture {
    fn drop(&mut self) {
        if let Err(e) = self.child.kill() {
            warn!("Failed to stop pw-record: {}", e);
        }
        let _ = self.child.wait();
        // Killing the process closes stdout, which ends the reader loop
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        info!("PipeWire system audio capture stopped");
    }
}
