// Linux system audio capture via the PulseAudio/PipeWire monitor source.
//
// WHY THIS EXISTS
// ---------------
// On Linux there is no "output device" that can be opened for capture. System
// audio is captured from a *monitor source* exposed by the sound server
// (PulseAudio or pipewire-pulse), e.g. `alsa_output.pci-0000_00_1f.3.analog-stereo.monitor`.
//
// cpal cannot reach those sources:
//   * cpal's ALSA host enumerates devices via `snd_device_name_hint()`, which
//     lists PCM plugins (`default`, `pulse`, `pipewire`, `hw:...`) but never
//     individual PulseAudio sources, so no monitor ever appears.
//   * cpal has no API to construct a `Device` from an arbitrary PCM name, and
//     it eagerly opens PCM handles during enumeration, so the `PULSE_SOURCE`
//     environment variable cannot be applied to just one stream.
//
// The ALSA `pulse` plugin accepts the source as an inline argument
// (`pulse:<source>`, see /usr/share/alsa/alsa.conf.d/50-pulseaudio.conf), so we
// open the PCM directly with the `alsa` crate — the same crate cpal itself uses.
// `@DEFAULT_MONITOR@` resolves to the monitor of the current default sink, so it
// follows the user's output device automatically.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use alsa::pcm::{Access, Format, HwParams, State, PCM};
use alsa::{Direction, ValueOr};
use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};

/// Friendly name surfaced to the UI and stored in recording metadata.
pub const DEFAULT_MONITOR_DEVICE_NAME: &str = "Default System Audio (Monitor)";

/// ALSA PCM that resolves to the monitor of the current default sink.
const DEFAULT_MONITOR_PCM: &str = "pulse:@DEFAULT_MONITOR@";

/// Sample rate requested from the monitor source. The sound server resamples
/// for us if the sink runs at a different rate.
const TARGET_SAMPLE_RATE: u32 = 48000;

/// Channel count requested from the monitor source.
const TARGET_CHANNELS: u32 = 2;

/// Frames read per `readi` call. ~21 ms at 48 kHz.
const FRAMES_PER_READ: usize = 1024;

/// Map an `AudioDevice` name to the ALSA PCM string used to open it.
///
/// Accepts:
/// * the friendly default name -> `pulse:@DEFAULT_MONITOR@`
/// * an explicit `pulse:...` / `pipewire:...` PCM -> passed through
/// * a raw PulseAudio source name ending in `.monitor` -> wrapped in `pulse:`
/// * anything else -> falls back to the default monitor
pub fn pcm_name_for(device_name: &str) -> String {
    let trimmed = device_name.trim();

    if trimmed.is_empty() || trimmed == DEFAULT_MONITOR_DEVICE_NAME || trimmed == "default" {
        return DEFAULT_MONITOR_PCM.to_string();
    }

    if trimmed.starts_with("pulse:") || trimmed.starts_with("pipewire:") {
        return trimmed.to_string();
    }

    if trimmed.ends_with(".monitor") || trimmed.starts_with('@') {
        return format!("pulse:{}", trimmed);
    }

    warn!(
        "🔊 [linux] Unrecognized system audio device '{}', falling back to the default monitor",
        trimmed
    );
    DEFAULT_MONITOR_PCM.to_string()
}

/// Check whether a monitor source can actually be opened on this machine.
///
/// Used to decide if system audio should be offered at all (a machine with no
/// running sound server, i.e. bare ALSA, has no monitor).
pub fn is_monitor_available() -> bool {
    match PCM::new(DEFAULT_MONITOR_PCM, Direction::Capture, false) {
        Ok(_) => true,
        Err(e) => {
            debug!(
                "🔊 [linux] Monitor source '{}' unavailable: {}",
                DEFAULT_MONITOR_PCM, e
            );
            false
        }
    }
}

/// Negotiated stream format for an opened monitor PCM.
#[derive(Debug, Clone, Copy)]
pub struct MonitorFormat {
    pub sample_rate: u32,
    pub channels: u16,
    /// True when the PCM delivers f32 samples, false when it delivers i16.
    pub is_float: bool,
}

/// Open the monitor PCM and negotiate hardware parameters.
fn open_monitor(pcm_name: &str) -> Result<(PCM, MonitorFormat)> {
    let pcm = PCM::new(pcm_name, Direction::Capture, false)
        .map_err(|e| anyhow!("Failed to open monitor PCM '{}': {}", pcm_name, e))?;

    let format = {
        let hwp = HwParams::any(&pcm)
            .map_err(|e| anyhow!("Failed to query hw params for '{}': {}", pcm_name, e))?;

        hwp.set_access(Access::RWInterleaved)
            .map_err(|e| anyhow!("Monitor '{}' rejected interleaved access: {}", pcm_name, e))?;

        // Prefer f32 (native pipeline format); fall back to S16 if unsupported.
        let is_float = match hwp.set_format(Format::float()) {
            Ok(()) => true,
            Err(_) => {
                hwp.set_format(Format::s16()).map_err(|e| {
                    anyhow!("Monitor '{}' supports neither f32 nor s16: {}", pcm_name, e)
                })?;
                false
            }
        };

        hwp.set_channels(TARGET_CHANNELS)
            .or_else(|_| hwp.set_channels(1))
            .map_err(|e| anyhow!("Monitor '{}' rejected channel count: {}", pcm_name, e))?;

        hwp.set_rate(TARGET_SAMPLE_RATE, ValueOr::Nearest)
            .map_err(|e| anyhow!("Monitor '{}' rejected sample rate: {}", pcm_name, e))?;

        // Keep latency bounded; non-fatal if the plugin ignores these.
        let _ = hwp.set_period_size_near(FRAMES_PER_READ as i64, ValueOr::Nearest);
        let _ = hwp.set_buffer_size_near((FRAMES_PER_READ * 4) as i64);

        let sample_rate = hwp
            .get_rate()
            .map_err(|e| anyhow!("Failed to read negotiated rate: {}", e))?;
        let channels = hwp
            .get_channels()
            .map_err(|e| anyhow!("Failed to read negotiated channels: {}", e))?;

        pcm.hw_params(&hwp)
            .map_err(|e| anyhow!("Failed to apply hw params to '{}': {}", pcm_name, e))?;

        MonitorFormat {
            sample_rate,
            channels: channels as u16,
            is_float,
        }
    };

    pcm.prepare()
        .map_err(|e| anyhow!("Failed to prepare monitor PCM '{}': {}", pcm_name, e))?;

    Ok((pcm, format))
}

/// A running monitor capture. Dropping (or calling [`stop`]) ends the thread.
pub struct LinuxMonitorCapture {
    pcm_name: String,
    format: MonitorFormat,
    stop_flag: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl LinuxMonitorCapture {
    /// Open the monitor source for `device_name` and stream samples to a callback.
    ///
    /// The PCM is opened first, then `build_callback` is invoked with the
    /// negotiated [`MonitorFormat`] so the caller can size its downstream
    /// pipeline correctly (sample rate and channel count are decided by the
    /// sound server, not by us).
    ///
    /// The returned callback receives **interleaved f32** frames with
    /// [`MonitorFormat::channels`] channels at [`MonitorFormat::sample_rate`] Hz.
    pub fn start<B, F>(device_name: &str, build_callback: B) -> Result<Self>
    where
        B: FnOnce(MonitorFormat) -> F,
        F: FnMut(&[f32]) + Send + 'static,
    {
        let pcm_name = pcm_name_for(device_name);
        info!(
            "🔊 [linux] Opening system audio monitor: '{}' (from device '{}')",
            pcm_name, device_name
        );

        let (pcm, format) = open_monitor(&pcm_name)?;

        info!(
            "✅ [linux] Monitor opened: {} Hz, {} ch, format: {}",
            format.sample_rate,
            format.channels,
            if format.is_float { "f32" } else { "s16" }
        );

        let mut on_samples = build_callback(format);

        let stop_flag = Arc::new(AtomicBool::new(false));
        let thread_stop = stop_flag.clone();
        let thread_pcm_name = pcm_name.clone();
        let channels = format.channels as usize;

        let thread = std::thread::Builder::new()
            .name("meetily-sysaudio".into())
            .spawn(move || {
                // `alsa::PCM` is not Send-safe to share, but it is fine to move
                // into this thread which exclusively owns it for its lifetime.
                let pcm = pcm;

                if let Err(e) = pcm.start() {
                    // Some plugins auto-start on first read; only warn.
                    debug!("🔊 [linux] PCM start returned {} (continuing)", e);
                }

                let mut float_buf = vec![0f32; FRAMES_PER_READ * channels];
                let mut int_buf = vec![0i16; FRAMES_PER_READ * channels];
                let mut converted = vec![0f32; FRAMES_PER_READ * channels];
                let mut consecutive_errors = 0u32;

                info!("✅ [linux] System audio capture thread started ({})", thread_pcm_name);

                while !thread_stop.load(Ordering::Relaxed) {
                    let read_result = if format.is_float {
                        match pcm.io_f32() {
                            Ok(io) => io.readi(&mut float_buf),
                            Err(e) => Err(e),
                        }
                    } else {
                        match pcm.io_i16() {
                            Ok(io) => io.readi(&mut int_buf),
                            Err(e) => Err(e),
                        }
                    };

                    match read_result {
                        Ok(frames) => {
                            consecutive_errors = 0;
                            let samples = frames * channels;
                            if samples == 0 {
                                continue;
                            }

                            if format.is_float {
                                on_samples(&float_buf[..samples]);
                            } else {
                                for (dst, &src) in
                                    converted[..samples].iter_mut().zip(int_buf[..samples].iter())
                                {
                                    *dst = src as f32 / i16::MAX as f32;
                                }
                                on_samples(&converted[..samples]);
                            }
                        }
                        Err(e) => {
                            if thread_stop.load(Ordering::Relaxed) {
                                break;
                            }

                            consecutive_errors += 1;

                            // Recover from xruns/suspends; this is expected when
                            // the default sink changes or the server reconfigures.
                            if pcm.try_recover(e, true).is_err() {
                                error!(
                                    "❌ [linux] Unrecoverable monitor read error on '{}': {}",
                                    thread_pcm_name, e
                                );
                                break;
                            }

                            if consecutive_errors == 1 {
                                warn!(
                                    "⚠️ [linux] Monitor xrun on '{}', recovering",
                                    thread_pcm_name
                                );
                            }

                            if consecutive_errors > 50 {
                                error!(
                                    "❌ [linux] Too many consecutive monitor errors on '{}', stopping",
                                    thread_pcm_name
                                );
                                break;
                            }

                            if pcm.state() != State::Running {
                                let _ = pcm.prepare();
                            }
                        }
                    }
                }

                let _ = pcm.drop();
                info!("⚠️ [linux] System audio capture thread ended ({})", thread_pcm_name);
            })
            .map_err(|e| anyhow!("Failed to spawn system audio capture thread: {}", e))?;

        Ok(Self {
            pcm_name,
            format,
            stop_flag,
            thread: Some(thread),
        })
    }

    pub fn format(&self) -> MonitorFormat {
        self.format
    }

    pub fn pcm_name(&self) -> &str {
        &self.pcm_name
    }

    /// Signal the capture thread to stop and wait for it to exit.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread.take() {
            if handle.join().is_err() {
                warn!("⚠️ [linux] System audio capture thread panicked during shutdown");
            }
        }
    }
}

impl Drop for LinuxMonitorCapture {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_default_names_to_default_monitor() {
        assert_eq!(pcm_name_for(DEFAULT_MONITOR_DEVICE_NAME), DEFAULT_MONITOR_PCM);
        assert_eq!(pcm_name_for("default"), DEFAULT_MONITOR_PCM);
        assert_eq!(pcm_name_for("   "), DEFAULT_MONITOR_PCM);
    }

    #[test]
    fn passes_through_explicit_pcm_names() {
        assert_eq!(pcm_name_for("pulse:my.source.monitor"), "pulse:my.source.monitor");
        assert_eq!(pcm_name_for("pipewire:foo"), "pipewire:foo");
    }

    #[test]
    fn wraps_raw_monitor_source_names() {
        assert_eq!(
            pcm_name_for("alsa_output.pci-0000_00_1f.3.analog-stereo.monitor"),
            "pulse:alsa_output.pci-0000_00_1f.3.analog-stereo.monitor"
        );
        assert_eq!(pcm_name_for("@DEFAULT_MONITOR@"), "pulse:@DEFAULT_MONITOR@");
    }

    #[test]
    fn unknown_names_fall_back_to_default_monitor() {
        assert_eq!(pcm_name_for("Built-in Audio Analog Stereo"), DEFAULT_MONITOR_PCM);
    }
}
