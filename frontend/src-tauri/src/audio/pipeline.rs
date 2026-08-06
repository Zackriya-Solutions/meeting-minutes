use std::sync::Arc;
use std::collections::VecDeque;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use anyhow::Result;
use log::{debug, error, info, warn};
use crate::batch_audio_metric;
use super::batch_processor::AudioMetricsBatcher;
use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};

use super::devices::AudioDevice;
use super::recording_state::{AudioChunk, AudioError, RecordingState, DeviceType};
use super::audio_processing::{audio_to_mono, LoudnessNormalizer, NoiseSuppressionProcessor, HighPassFilter};

/// Ring buffer for synchronized audio mixing
/// Accumulates samples from mic and system streams until we have aligned windows
struct AudioMixerRingBuffer {
    mic_buffer: VecDeque<f32>,
    system_buffer: VecDeque<f32>,
    window_size_samples: usize,  // Fixed mixing window (e.g., 50ms)
    max_buffer_size: usize,  // Safety limit (e.g., 100ms)
    // ponytail: rate diagnostics. The two streams are supposed to deliver
    // `sample_rate` samples per second each; when one doesn't, the mix drifts.
    // Padding is now only applied to streams that have gone silent, so any
    // padding here means a stream stalled or never started.
    sample_rate: u32,
    started: std::time::Instant,
    mic_in: u64,
    sys_in: u64,
    mic_pad: u64,
    sys_pad: u64,
    windows: u64,
    // Last time each stream delivered anything. A stream that is still
    // delivering must never be zero-padded (see can_mix); one that has gone
    // quiet must never stall the mix.
    mic_last: Option<std::time::Instant>,
    sys_last: Option<std::time::Instant>,
}

impl AudioMixerRingBuffer {
    fn new(sample_rate: u32) -> Self {
        // Use 50ms windows for mixing
        let window_ms = 600.0;
        let window_size_samples = (sample_rate as f32 * window_ms / 1000.0) as usize;

        // CRITICAL FIX: Increase max buffer to 400ms for system audio stability
        // System audio (especially Core Audio on macOS) can have significant jitter
        // due to sample-by-sample streaming → batching → channel transmission
        // Accounts for: RNNoise buffering + Core Audio jitter + processing delays
        let max_buffer_size = window_size_samples * 8;  // 400ms (was 200ms)

        info!("🔊 Ring buffer initialized: window={}ms ({} samples), max={}ms ({} samples)",
              window_ms, window_size_samples,
              window_ms * 8.0, max_buffer_size);

        Self {
            mic_buffer: VecDeque::with_capacity(max_buffer_size),
            system_buffer: VecDeque::with_capacity(max_buffer_size),
            window_size_samples,
            max_buffer_size,
            sample_rate,
            started: std::time::Instant::now(),
            mic_in: 0,
            sys_in: 0,
            mic_pad: 0,
            sys_pad: 0,
            windows: 0,
            mic_last: None,
            sys_last: None,
        }
    }

    /// A stream is "live" while it is still handing us audio. Anything quieter
    /// than this for longer is treated as absent (no device, stopped stream,
    /// disconnected headset) and mixed as silence rather than waited for.
    /// Generous on purpose: capture callbacks arrive every 10-85ms, so a full
    /// second of nothing means the stream really is gone, not just jittery.
    const STREAM_IDLE_AFTER: std::time::Duration = std::time::Duration::from_secs(1);

    fn is_live(last: Option<std::time::Instant>) -> bool {
        last.is_some_and(|t| t.elapsed() < Self::STREAM_IDLE_AFTER)
    }

    /// Effective delivery rate of each stream vs. the rate the mixer assumes.
    fn log_rates(&self) {
        let secs = self.started.elapsed().as_secs_f64();
        if secs <= 0.0 {
            return;
        }
        let expected = self.sample_rate as f64;
        let win = self.windows.max(1) as f64;
        info!(
            "📐 Mix rates after {:.1}s / {} windows: mic {:.0} Hz ({:.0}% of {:.0}), \
             sys {:.0} Hz ({:.0}%), padding per window: mic {:.0}ms, sys {:.0}ms, \
             buffered: mic {} / sys {}",
            secs,
            self.windows,
            self.mic_in as f64 / secs,
            self.mic_in as f64 / secs / expected * 100.0,
            expected,
            self.sys_in as f64 / secs,
            self.sys_in as f64 / secs / expected * 100.0,
            self.mic_pad as f64 / win / expected * 1000.0,
            self.sys_pad as f64 / win / expected * 1000.0,
            self.mic_buffer.len(),
            self.system_buffer.len(),
        );
    }

    fn add_samples(&mut self, device_type: DeviceType, samples: Vec<f32>) {
        // Log buffer health periodically for diagnostics
        static mut SAMPLE_COUNTER: u64 = 0;
        unsafe {
            SAMPLE_COUNTER += 1;
            if SAMPLE_COUNTER % 200 == 0 {
                debug!("📊 Ring buffer status: mic={} samples, sys={} samples (max={})",
                       self.mic_buffer.len(), self.system_buffer.len(), self.max_buffer_size);
            }
        }

        match device_type {
            DeviceType::Microphone => {
                self.mic_in += samples.len() as u64;
                self.mic_last = Some(std::time::Instant::now());
                self.mic_buffer.extend(samples);
            }
            DeviceType::System => {
                self.sys_in += samples.len() as u64;
                self.sys_last = Some(std::time::Instant::now());
                self.system_buffer.extend(samples);
            }
        }

        // CRITICAL FIX: Add warnings before dropping samples
        // This helps diagnose timing issues in production
        if self.mic_buffer.len() > self.max_buffer_size {
            warn!("⚠️ Microphone buffer overflow: {} > {} samples, dropping oldest {} samples",
                  self.mic_buffer.len(), self.max_buffer_size,
                  self.mic_buffer.len() - self.max_buffer_size);
        }
        if self.system_buffer.len() > self.max_buffer_size {
            error!("🔴 SYSTEM AUDIO BUFFER OVERFLOW: {} > {} samples, dropping {} samples - THIS CAUSES DISTORTION!",
                  self.system_buffer.len(), self.max_buffer_size,
                  self.system_buffer.len() - self.max_buffer_size);
        }

        // Safety: prevent buffer overflow (keep only last 200ms)
        while self.mic_buffer.len() > self.max_buffer_size {
            self.mic_buffer.pop_front();
        }
        while self.system_buffer.len() > self.max_buffer_size {
            self.system_buffer.pop_front();
        }
    }

    /// A window is mixable once one stream has a full window AND no *live*
    /// stream is short of one.
    ///
    /// Mixing on `||` alone is what dismembers recordings: whenever the two
    /// streams deliver at different rates (different clocks, different
    /// latency, or an over-delivering Core Audio tap), the faster one hits a
    /// full window while the slower one is part-filled, and extract_window
    /// zero-pads the rest. The slower stream's audio then arrives as
    /// "N ms of speech, silence to the window edge", forever, and the file
    /// grows longer than the meeting. Waiting for a live stream costs nothing:
    /// its samples are already on their way.
    fn can_mix(&self) -> bool {
        let mic_full = self.mic_buffer.len() >= self.window_size_samples;
        let sys_full = self.system_buffer.len() >= self.window_size_samples;

        (mic_full || sys_full)
            && (mic_full || !Self::is_live(self.mic_last))
            && (sys_full || !Self::is_live(self.sys_last))
    }

    fn extract_window(&mut self) -> Option<(Vec<f32>, Vec<f32>)> {
        if !self.can_mix() {
            return None;
        }

        // Extract mic window with zero-padding for incomplete buffers
        // Zero-padding (silence) is preferred over last-sample-hold to prevent artifacts

        // Extract mic window (or pad with zeros if insufficient data)
        let mic_window = if self.mic_buffer.len() >= self.window_size_samples {
            // Enough mic data - drain window
            self.mic_buffer.drain(0..self.window_size_samples).collect()
        } else if !self.mic_buffer.is_empty() {
            // Some mic data but not enough - consume all + pad with zeros
            let available: Vec<f32> = self.mic_buffer.drain(..).collect();
            self.mic_pad += (self.window_size_samples - available.len()) as u64;
            let mut padded = Vec::with_capacity(self.window_size_samples);
            padded.extend_from_slice(&available);

            // Use zero-padding (silence) to prevent repetition artifacts
            // Zero-padding is inaudible at 48kHz sample rate
            padded.resize(self.window_size_samples, 0.0);

            padded
        } else {
            // No mic data - return silence
            self.mic_pad += self.window_size_samples as u64;
            vec![0.0; self.window_size_samples]
        };

        // Extract system window (or pad with zeros if insufficient data)
        let sys_window = if self.system_buffer.len() >= self.window_size_samples {
            // Enough system data - drain window
            self.system_buffer.drain(0..self.window_size_samples).collect()
        } else if !self.system_buffer.is_empty() {
            // Some system data but not enough - consume all + pad with zeros
            let available: Vec<f32> = self.system_buffer.drain(..).collect();
            self.sys_pad += (self.window_size_samples - available.len()) as u64;
            let mut padded = Vec::with_capacity(self.window_size_samples);
            padded.extend_from_slice(&available);

            // Use zero-padding (silence) to prevent repetition artifacts
            // Zero-padding is inaudible at 48kHz sample rate
            padded.resize(self.window_size_samples, 0.0);

            padded
        } else {
            // No system data - return silence
            self.sys_pad += self.window_size_samples as u64;
            vec![0.0; self.window_size_samples]
        };

        self.windows += 1;
        if self.windows % 8 == 0 {
            self.log_rates();
        }

        Some((mic_window, sys_window))
    }

}

/// Simple audio mixer without aggressive ducking
/// Combines mic + system audio with basic clipping prevention
struct ProfessionalAudioMixer;

impl ProfessionalAudioMixer {
    fn new(_sample_rate: u32) -> Self {
        Self
    }

    fn mix_window(&mut self, mic_window: &[f32], sys_window: &[f32]) -> Vec<f32> {
        // Handle different lengths (already padded by extract_window, but defensive)
        let max_len = mic_window.len().max(sys_window.len());
        let mut mixed = Vec::with_capacity(max_len);

        // Professional mixing with soft scaling to prevent distortion
        // Uses proportional scaling instead of hard clamping to avoid artifacts
        for i in 0..max_len {
            let mic = mic_window.get(i).copied().unwrap_or(0.0);
            let sys = sys_window.get(i).copied().unwrap_or(0.0);

            // Pre-scale system audio to 70% to leave headroom
            // This prevents constant soft scaling which can cause pumping artifacts
            // Mic is normalized to -23 LUFS (already optimal), system needs reduction
            let sys_scaled = sys * 1.0;
            let _mic_scaled = mic * 0.8;  // Reserved for future mic scaling

            // Sum without ducking - mic stays at full volume, system slightly reduced
            let sum = mic + sys_scaled;

            // CRITICAL FIX: Soft scaling prevents distortion artifacts
            // If the sum would exceed ±1.0, scale down PROPORTIONALLY
            // This avoids hard clipping distortion that sounds like "radio breaks"
            let sum_abs = sum.abs();
            let mixed_sample = if sum_abs > 1.0 {
                // Scale down to fit within ±1.0
                sum / sum_abs
            } else {
                sum
            };

            mixed.push(mixed_sample);
        }

        mixed
    }
}

/// Simplified audio capture without broadcast channels
#[derive(Clone)]
pub struct AudioCapture {
    device: Arc<AudioDevice>,
    state: Arc<RecordingState>,
    sample_rate: u32,        // Original device sample rate
    channels: u16,
    chunk_counter: Arc<std::sync::atomic::AtomicU64>,
    device_type: DeviceType,
    recording_sender: Option<mpsc::UnboundedSender<AudioChunk>>,
    needs_resampling: bool,  // Flag if resampling is required
    // CRITICAL FIX: Persistent resampler to preserve energy across chunks
    resampler: Arc<std::sync::Mutex<Option<SincFixedIn<f32>>>>,
    // Buffering for variable-size chunks → fixed-size resampler input
    resampler_input_buffer: Arc<std::sync::Mutex<Vec<f32>>>,
    resampler_chunk_size: usize,  // Fixed chunk size for resampler (512 samples)
    // Audio enhancement processors (microphone only)
    noise_suppressor: Arc<std::sync::Mutex<Option<NoiseSuppressionProcessor>>>,
    high_pass_filter: Arc<std::sync::Mutex<Option<HighPassFilter>>>,
    // EBU R128 normalizer for microphone audio (per-device, stateful)
    normalizer: Arc<std::sync::Mutex<Option<LoudnessNormalizer>>>,
    // Note: Using global recording timestamp for synchronization
    // ponytail: capture-callback health. Says whether a stream that delivers
    // below its nominal rate is being starved by the device or by us blowing
    // the callback deadline.
    started: std::time::Instant,
    callbacks: Arc<std::sync::atomic::AtomicU64>,
    raw_frames: Arc<std::sync::atomic::AtomicU64>,
    busy_micros: Arc<std::sync::atomic::AtomicU64>,
}

/// Adds this callback's runtime to the total on every exit path.
struct BusyTimer<'a> {
    start: std::time::Instant,
    total: &'a std::sync::atomic::AtomicU64,
}

impl Drop for BusyTimer<'_> {
    fn drop(&mut self) {
        self.total.fetch_add(
            self.start.elapsed().as_micros() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
    }
}

impl AudioCapture {
    pub fn new(
        device: Arc<AudioDevice>,
        state: Arc<RecordingState>,
        sample_rate: u32,
        channels: u16,
        device_type: DeviceType,
        recording_sender: Option<mpsc::UnboundedSender<AudioChunk>>,
    ) -> Self {
        // CRITICAL FIX: Detect if resampling is needed
        // Pipeline expects 48kHz, but Bluetooth devices often report 8kHz, 16kHz, or 44.1kHz
        const TARGET_SAMPLE_RATE: u32 = 48000;
        let needs_resampling = sample_rate != TARGET_SAMPLE_RATE;

        // Detect device kind (Bluetooth vs Wired) for adaptive processing
        // Use reasonable defaults for buffer size (512 samples is typical)
        let device_kind = super::device_detection::InputDeviceKind::detect(&device.name, 512, sample_rate);

        if needs_resampling {
            warn!(
                "⚠️ SAMPLE RATE MISMATCH DETECTED ⚠️"
            );
            warn!(
                "🔄 [{:?}] Audio device '{}' ({:?}) reports {} Hz (pipeline expects {} Hz)",
                device_type, device.name, device_kind, sample_rate, TARGET_SAMPLE_RATE
            );
            warn!(
                "🔄 Automatic resampling will be applied: {} Hz → {} Hz",
                sample_rate, TARGET_SAMPLE_RATE
            );

            // Log which resampling strategy will be used
            let ratio = TARGET_SAMPLE_RATE as f64 / sample_rate as f64;
            let strategy = if ratio >= 2.0 {
                "High-quality upsampling (sinc_len=512, Cubic interpolation)"
            } else if ratio >= 1.5 {
                "Moderate upsampling (sinc_len=384, Cubic)"
            } else if ratio > 1.0 {
                "Small upsampling (sinc_len=256, Linear)"
            } else if ratio <= 0.5 {
                "Anti-aliased downsampling (sinc_len=512, Cubic)"
            } else {
                "Moderate downsampling (sinc_len=384, Linear)"
            };
            info!("   Resampling strategy: {}", strategy);
        } else {
            info!(
                "✅ [{:?}] Audio device '{}' ({:?}) uses {} Hz (matches pipeline)",
                device_type, device.name, device_kind, sample_rate
            );
        }

        // Initialize audio enhancement processors for MICROPHONE ONLY
        // System audio doesn't need enhancement (already clean)
        let (noise_suppressor, high_pass_filter, normalizer) = if matches!(device_type, DeviceType::Microphone) {
            // Initialize noise suppression (RNNoise) at 48kHz - CONDITIONAL based on flag
            let ns = if super::ffmpeg_mixer::RNNOISE_APPLY_ENABLED {
                match NoiseSuppressionProcessor::new(TARGET_SAMPLE_RATE) {
                    Ok(processor) => {
                        info!("✅ RNNoise noise suppression ENABLED for microphone '{}' (10-15 dB reduction)", device.name);
                        Some(processor)
                    }
                    Err(e) => {
                        warn!("⚠️ Failed to create noise suppressor: {}, continuing without noise suppression", e);
                        None
                    }
                }
            } else {
                info!("ℹ️ RNNoise noise suppression DISABLED for microphone '{}' (flag: RNNOISE_APPLY_ENABLED=false)", device.name);
                info!("   Whisper handles noise well internally - RNNoise is optional");
                None
            };

            // Initialize high-pass filter (removes rumble below 80 Hz)
            let hpf = {
                let filter = HighPassFilter::new(TARGET_SAMPLE_RATE, 80.0);
                info!("✅ High-pass filter initialized for microphone '{}' (cutoff: 80 Hz)", device.name);
                Some(filter)
            };

            // Initialize EBU R128 normalizer (professional loudness standard)
            let norm = match LoudnessNormalizer::new(1, TARGET_SAMPLE_RATE) {
                Ok(normalizer) => {
                    info!("✅ EBU R128 normalizer initialized for microphone '{}' (target: -23 LUFS)", device.name);
                    Some(normalizer)
                }
                Err(e) => {
                    warn!("⚠️ Failed to create normalizer for microphone: {}, normalization disabled", e);
                    None
                }
            };

            (ns, hpf, norm)
        } else {
            // System audio: no enhancement needed
            info!("ℹ️ System audio '{}' captured raw (no enhancement)", device.name);
            (None, None, None)
        };

        // CRITICAL FIX: Initialize persistent resampler to preserve energy across chunks
        // Creating a new resampler per chunk causes energy amplification and incorrect output sizes
        // Use fixed chunk size of 512 samples with buffering for variable-size input
        const RESAMPLER_CHUNK_SIZE: usize = 512;

        let resampler = if needs_resampling {
            let ratio = TARGET_SAMPLE_RATE as f64 / sample_rate as f64;

            // This runs INSIDE the realtime capture callback, which has one
            // buffer period (~7-20ms) to finish. The old settings scaled up to
            // sinc_len=512 with oversampling=512 — a 1MB filter table walked
            // per output sample. Overrun the deadline and CoreAudio does not
            // wait, it drops the next input buffer, so the device appears to
            // deliver at half its rate. 64 taps is transparent for speech.
            let params = SincInterpolationParameters {
                sinc_len: 64,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 128,
                window: WindowFunction::BlackmanHarris2,
            };

            match SincFixedIn::<f32>::new(
                ratio,
                2.0,  // Maximum relative deviation
                params,
                RESAMPLER_CHUNK_SIZE,
                1,    // Mono
            ) {
                Ok(resampler) => {
                    info!("✅ Persistent resampler initialized for '{}' ({}Hz → {}Hz, chunk_size={})",
                          device.name, sample_rate, TARGET_SAMPLE_RATE, RESAMPLER_CHUNK_SIZE);
                    info!("   Buffering enabled for variable-size chunks (e.g., 320, 512, 1024, etc.)");
                    Some(resampler)
                }
                Err(e) => {
                    warn!("⚠️ Failed to create persistent resampler: {}, will use fallback", e);
                    None
                }
            }
        } else {
            None
        };

        Self {
            device,
            state,
            sample_rate,
            channels,
            chunk_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            device_type,
            recording_sender,
            needs_resampling,
            resampler: Arc::new(std::sync::Mutex::new(resampler)),
            resampler_input_buffer: Arc::new(std::sync::Mutex::new(Vec::with_capacity(RESAMPLER_CHUNK_SIZE * 2))),
            resampler_chunk_size: RESAMPLER_CHUNK_SIZE,
            noise_suppressor: Arc::new(std::sync::Mutex::new(noise_suppressor)),
            high_pass_filter: Arc::new(std::sync::Mutex::new(high_pass_filter)),
            normalizer: Arc::new(std::sync::Mutex::new(normalizer)),
            // Using global recording time for sync
            started: std::time::Instant::now(),
            callbacks: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            raw_frames: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            busy_micros: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// How much audio the device actually handed us, and how much of the
    /// realtime budget we spent taking it. A device rate well under nominal
    /// with a high busy share means we are missing callback deadlines and
    /// CoreAudio is dropping input buffers; a low busy share means the device
    /// itself is starving us.
    fn log_capture_health(&self) {
        use std::sync::atomic::Ordering::Relaxed;

        let secs = self.started.elapsed().as_secs_f64();
        if secs <= 0.0 {
            return;
        }
        let calls = self.callbacks.load(Relaxed).max(1);
        let raw_hz = self.raw_frames.load(Relaxed) as f64 / secs;
        let busy = self.busy_micros.load(Relaxed) as f64;

        info!(
            "🎚️ [{:?}] '{}': {:.0} Hz raw ({:.0}% of the {} Hz it reports), \
             {} callbacks in {:.1}s, callback busy {:.0}% of realtime ({:.2}ms avg)",
            self.device_type,
            self.device.name,
            raw_hz,
            raw_hz / self.sample_rate as f64 * 100.0,
            self.sample_rate,
            calls,
            secs,
            busy / (secs * 1_000_000.0) * 100.0,
            busy / calls as f64 / 1000.0,
        );
    }

    /// Process audio data directly from callback
    pub fn process_audio_data(&self, data: &[f32]) {
        // Check if still recording
        if !self.state.is_recording() {
            return;
        }

        use std::sync::atomic::Ordering::Relaxed;
        let _busy = BusyTimer { start: std::time::Instant::now(), total: &self.busy_micros };
        self.raw_frames
            .fetch_add((data.len() / self.channels.max(1) as usize) as u64, Relaxed);
        if self.callbacks.fetch_add(1, Relaxed) % 200 == 199 {
            self.log_capture_health();
        }

        // Convert to mono if needed
        let mut mono_data = if self.channels > 1 {
            audio_to_mono(data, self.channels)
        } else {
            data.to_vec()
        };

        // CRITICAL FIX: Resample to 48kHz if device uses different sample rate
        // This fixes Bluetooth devices (like Sony WH-1000XM4) that report 16kHz or 44.1kHz
        // Without this, audio is sped up 3x and VAD fails
        //
        // IMPORTANT: Uses PERSISTENT resampler with BUFFERING to preserve energy across chunks
        // Creating a new resampler per chunk causes energy amplification (173.5% RMS)
        // Buffering handles variable chunk sizes (320, 512, 1024, etc.) by accumulating to fixed 512-sample chunks
        const TARGET_SAMPLE_RATE: u32 = 48000;
        if self.needs_resampling {
            let before_len = mono_data.len();
            let before_rms = if !mono_data.is_empty() {
                (mono_data.iter().map(|&x| x * x).sum::<f32>() / mono_data.len() as f32).sqrt()
            } else {
                0.0
            };

            // Use persistent resampler with buffering to handle variable chunk sizes
            let mut resampled_output = Vec::new();
            let mut used_persistent_resampler = false;

            if let Ok(mut buffer_lock) = self.resampler_input_buffer.lock() {
                // Add new samples to buffer
                buffer_lock.extend_from_slice(&mono_data);

                // Process complete chunks through the resampler
                if let Ok(mut resampler_lock) = self.resampler.lock() {
                    if let Some(ref mut resampler) = *resampler_lock {
                        used_persistent_resampler = true;

                        // Process as many complete chunks as we have
                        while buffer_lock.len() >= self.resampler_chunk_size {
                            // Extract exactly chunk_size samples
                            let chunk: Vec<f32> = buffer_lock.drain(0..self.resampler_chunk_size).collect();

                            // Rubato expects input as Vec<Vec<f32>> (one Vec per channel)
                            let waves_in = vec![chunk];

                            match resampler.process(&waves_in, None) {
                                Ok(mut waves_out) => {
                                    if let Some(output) = waves_out.pop() {
                                        resampled_output.extend_from_slice(&output);
                                    }
                                }
                                Err(e) => {
                                    warn!("⚠️ Persistent resampler processing failed: {}", e);
                                    used_persistent_resampler = false;
                                    break;
                                }
                            }
                        }
                        // Remaining samples in buffer will be processed in next iteration
                    }
                }
            }

            // CRITICAL: Only update mono_data if we got output from persistent resampler
            // If buffer is accumulating (< 512 samples), skip this chunk - data is safely buffered
            // and will be processed in next iteration with proper resampling
            let has_resampled_output = !resampled_output.is_empty();

            if has_resampled_output {
                mono_data = resampled_output;
            } else if !used_persistent_resampler {
                // Only fallback if persistent resampler is not available at all
                mono_data = super::audio_processing::resample_audio(
                    &mono_data,
                    self.sample_rate,
                    TARGET_SAMPLE_RATE,
                );
            } else {
                // Buffering: samples are accumulating in buffer, waiting for 512-sample chunk
                // Don't send partial/unprocessed data - return early
                // Audio is NOT lost - it's in the buffer and will be processed next iteration
                return;
            }

            // Log resampling only occasionally to avoid spam
            let chunk_id = self.chunk_counter.load(std::sync::atomic::Ordering::SeqCst);
            if chunk_id % 100 == 0 && has_resampled_output {
                let after_len = mono_data.len();
                let after_rms = if !mono_data.is_empty() {
                    (mono_data.iter().map(|&x| x * x).sum::<f32>() / mono_data.len() as f32).sqrt()
                } else {
                    0.0
                };
                let ratio = TARGET_SAMPLE_RATE as f64 / self.sample_rate as f64;
                let rms_preservation = if before_rms > 0.0 { (after_rms / before_rms) * 100.0 } else { 100.0 };

                let buffer_size = if let Ok(buf) = self.resampler_input_buffer.lock() {
                    buf.len()
                } else {
                    0
                };

                info!(
                    "🔄 [{:?}] Persistent buffered resampler: {}Hz → {}Hz (ratio: {:.2}x)",
                    self.device_type,
                    self.sample_rate,
                    TARGET_SAMPLE_RATE,
                    ratio
                );
                info!(
                    "   Chunk {}: {} → {} samples, RMS preservation: {:.1}%, buffer: {}",
                    chunk_id,
                    before_len,
                    after_len,
                    rms_preservation,
                    buffer_size
                );
            }
        }

        // AUDIO ENHANCEMENT PIPELINE (Microphone Only)
        // Processing order is critical: high-pass → noise suppression → normalization
        // This ensures noise is removed before being amplified by the normalizer
        if matches!(self.device_type, DeviceType::Microphone) {
            // STEP 1: Apply high-pass filter to remove low-frequency rumble (< 80 Hz)
            if let Ok(mut hpf_lock) = self.high_pass_filter.lock() {
                if let Some(ref mut filter) = *hpf_lock {
                    mono_data = filter.process(&mono_data);
                }
            }

            // STEP 2: Apply RNNoise noise suppression (10-15 dB reduction) - CONDITIONAL
            if super::ffmpeg_mixer::RNNOISE_APPLY_ENABLED {
                if let Ok(mut ns_lock) = self.noise_suppressor.lock() {
                    if let Some(ref mut suppressor) = *ns_lock {
                        let before_len = mono_data.len();
                        mono_data = suppressor.process(&mono_data);
                        let after_len = mono_data.len();

                        // CRITICAL MONITORING: Track buffer health
                        let chunk_id = self.chunk_counter.load(std::sync::atomic::Ordering::SeqCst);
                        if chunk_id % 100 == 0 {
                            let buffered = suppressor.buffered_samples();
                            let length_delta = (before_len as i32 - after_len as i32).abs();

                            debug!("🔇 Noise suppression health: in={}, out={}, delta={}, buffered={}, RMS={:.4}",
                                   before_len, after_len, length_delta, buffered,
                                   if !mono_data.is_empty() {
                                       (mono_data.iter().map(|&x| x * x).sum::<f32>() / mono_data.len() as f32).sqrt()
                                   } else { 0.0 });

                            // WARN if accumulating samples (potential latency buildup)
                            if buffered > 1000 {
                                warn!("⚠️ RNNoise accumulating samples: {} buffered (potential latency issue!)",
                                      buffered);
                            }

                            // WARN if significant length mismatch
                            if length_delta > 50 {
                                warn!("⚠️ RNNoise length mismatch: input={} output={} (delta={})",
                                      before_len, after_len, length_delta);
                            }
                        }
                    }
                }
            }

            // STEP 3: Apply EBU R128 normalization (professional loudness standard)
            if let Ok(mut normalizer_lock) = self.normalizer.lock() {
                if let Some(ref mut normalizer) = *normalizer_lock {
                    mono_data = normalizer.normalize_loudness(&mono_data);

                    // Log normalization occasionally for debugging
                    let chunk_id = self.chunk_counter.load(std::sync::atomic::Ordering::SeqCst);
                    if chunk_id % 200 == 0 && !mono_data.is_empty() {
                        let rms = (mono_data.iter().map(|&x| x * x).sum::<f32>() / mono_data.len() as f32).sqrt();
                        let peak = mono_data.iter().map(|&x| x.abs()).fold(0.0f32, f32::max);
                        debug!("🎤 After normalization chunk {}: RMS={:.4}, Peak={:.4}", chunk_id, rms, peak);
                    }
                }
            }
        }

        // Create audio chunk with stream-specific timestamp (get ID first for logging)
        let chunk_id = self.chunk_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // RAW AUDIO: No gain applied here - will be applied AFTER mixing
        // This prevents amplifying system audio bleed-through in the microphone

        // DIAGNOSTIC: Log audio levels for debugging (especially mic issues)
        // if chunk_id % 100 == 0 && !mono_data.is_empty() {
        //     let raw_rms = (mono_data.iter().map(|&x| x * x).sum::<f32>() / mono_data.len() as f32).sqrt();
        //     let raw_peak = mono_data.iter().map(|&x| x.abs()).fold(0.0f32, f32::max);

        //         info!("🎙️ [{:?}] Chunk {} - Raw: RMS={:.6}, Peak={:.6}",
        //               self.device_type, chunk_id, raw_rms, raw_peak);

        //     // Warn if microphone is completely silent
        //     if matches!(self.device_type, DeviceType::Microphone) && raw_rms == 0.0 && raw_peak == 0.0 {
        //         warn!("⚠️ Microphone producing ZERO audio - check permissions or hardware!");
        //     }
        // }
        // else if chunk_id % 100 == 0 && matches!(self.device_type, DeviceType::System) {
        //     let raw_rms = (mono_data.iter().map(|&x| x * x).sum::<f32>() / mono_data.len() as f32).sqrt();
        //     let raw_peak = mono_data.iter().map(|&x| x.abs()).fold(0.0f32, f32::max);
        //     info!("🔊 [{:?}] Chunk {} - Raw: RMS={:.6}, Peak={:.6}",
        //       self.device_type, chunk_id, raw_rms, raw_peak);
            
        //     // Warn if system audio is completely silent
        //     if raw_rms == 0.0 && raw_peak == 0.0 {
        //         warn!("⚠️ System audio producing ZERO audio - check permissions or hardware!");
        //     }
        // }

        // Use global recording timestamp for proper synchronization
        let timestamp = self.state.get_recording_duration().unwrap_or(0.0);

        // RAW AUDIO CHUNK: No gain applied - will be mixed and gained downstream
        // Use 48kHz if we resampled, otherwise use original rate
        let audio_chunk = AudioChunk {
            data: mono_data,  // Raw audio (resampled if needed), no gain yet
            sample_rate: if self.needs_resampling { 48000 } else { self.sample_rate },
            timestamp,
            chunk_id,
            device_type: self.device_type.clone(),
        };

        // NOTE: Raw audio is NOT sent to recording saver to prevent echo
        // Only the mixed audio (from AudioPipeline) is saved to file (see pipeline.rs:726-736)
        // This ensures we only record once: mic + system properly mixed
        // Individual raw streams go only to the transcription pipeline below

        // Send to processing pipeline for transcription
        if let Err(e) = self.state.send_audio_chunk(audio_chunk) {
            // Check if this is the "pipeline not ready" error
            if e.to_string().contains("Audio pipeline not ready") {
                // This is expected during initialization, just log it as debug
                debug!("Audio pipeline not ready yet, skipping chunk {}", chunk_id);
                return;
            }

            warn!("Failed to send audio chunk: {}", e);
            // More specific error handling based on failure reason
            let error = if e.to_string().contains("channel closed") {
                AudioError::ChannelClosed
            } else if e.to_string().contains("full") {
                AudioError::BufferOverflow
            } else {
                AudioError::ProcessingFailed
            };
            self.state.report_error(error);
        } else {
            debug!("Sent audio chunk {} ({} samples)", chunk_id, data.len());
        }
    }

    /// Handle stream errors with enhanced disconnect detection
    pub fn handle_stream_error(&self, error: cpal::StreamError) {
        error!("Audio stream error for {}: {}", self.device.name, error);

        let error_str = error.to_string().to_lowercase();

        // Enhanced error detection for device disconnection
        let audio_error = if error_str.contains("device is no longer available")
            || error_str.contains("device not found")
            || error_str.contains("device disconnected")
            || error_str.contains("no such device")
            || error_str.contains("device unavailable")
            || error_str.contains("device removed")
        {
            warn!("🔌 Device disconnect detected for: {}", self.device.name);
            AudioError::DeviceDisconnected
        } else if error_str.contains("permission") || error_str.contains("access denied") {
            AudioError::PermissionDenied
        } else if error_str.contains("channel closed") {
            AudioError::ChannelClosed
        } else if error_str.contains("stream") && error_str.contains("failed") {
            AudioError::StreamFailed
        } else {
            warn!("Unknown audio error: {}", error);
            AudioError::StreamFailed
        };

        self.state.report_error(audio_error);
    }
}

/// VAD-driven audio processing pipeline
/// Uses Voice Activity Detection to segment speech in real-time and send only speech to Whisper
pub struct AudioPipeline {
    receiver: mpsc::UnboundedReceiver<AudioChunk>,
    transcription_sender: mpsc::UnboundedSender<AudioChunk>,
    state: Arc<RecordingState>,
    // Audio forwarded to the transcription stream so far, in 16kHz samples.
    // Drives recording-relative timestamps without a wall clock.
    transcription_samples_sent: u64,
    sample_rate: u32,
    chunk_id_counter: u64,
    // Performance optimization: reduce logging frequency
    last_summary_time: std::time::Instant,
    processed_chunks: u64,
    // Smart batching for audio metrics
    metrics_batcher: Option<AudioMetricsBatcher>,
    // PROFESSIONAL AUDIO MIXING: Ring buffer + RMS-based mixer
    ring_buffer: AudioMixerRingBuffer,
    mixer: ProfessionalAudioMixer,
    // Recording sender for pre-mixed audio
    recording_sender_for_mixed: Option<mpsc::UnboundedSender<AudioChunk>>,
}

impl AudioPipeline {
    pub fn new(
        receiver: mpsc::UnboundedReceiver<AudioChunk>,
        transcription_sender: mpsc::UnboundedSender<AudioChunk>,
        state: Arc<RecordingState>,
        target_chunk_duration_ms: u32,
        sample_rate: u32,
        mic_device_name: String,
        mic_device_kind: super::device_detection::InputDeviceKind,
        system_device_name: String,
        system_device_kind: super::device_detection::InputDeviceKind,
    ) -> Self {
        // Log device characteristics for adaptive buffering
        info!("🎛️ AudioPipeline initializing with device characteristics:");
        info!("   Mic: '{}' ({:?}) - Buffer: {:?}",
              mic_device_name, mic_device_kind, mic_device_kind.buffer_timeout());
        info!("   System: '{}' ({:?}) - Buffer: {:?}",
              system_device_name, system_device_kind, system_device_kind.buffer_timeout());

        // Device kind information can be used for adaptive buffering in the future
        // For now, we log it for monitoring and potential optimization
        let _ = (mic_device_name, mic_device_kind, system_device_name, system_device_kind);

        // No VAD here any more: the transcription engine keeps one continuous
        // stream open and does its own endpointing. VAD is still used by the
        // file-import and retranscription paths, which do need segmentation.
        info!("Pipeline: mixed audio is forwarded continuously to the transcription stream");

        // Initialize professional audio mixing components
        let ring_buffer = AudioMixerRingBuffer::new(sample_rate);
        let mixer = ProfessionalAudioMixer::new(sample_rate);

        // Note: target_chunk_duration_ms is ignored - VAD controls segmentation now
        let _ = target_chunk_duration_ms;

        Self {
            receiver,
            transcription_sender,
            state,
            transcription_samples_sent: 0,
            sample_rate,
            chunk_id_counter: 0,
            // Performance optimization: reduce logging frequency
            last_summary_time: std::time::Instant::now(),
            processed_chunks: 0,
            // Initialize metrics batcher for smart batching
            metrics_batcher: Some(AudioMetricsBatcher::new()),
            // Initialize professional audio mixing
            ring_buffer,
            mixer,
            recording_sender_for_mixed: None,  // Will be set by manager
        }
    }

    /// Run the VAD-driven audio processing pipeline
    pub async fn run(mut self) -> Result<()> {
        info!("VAD-driven audio pipeline started - segments sent in real-time based on speech detection");

        // Latched once the transcription receiver is dropped, so its warning is
        // logged once per recording instead of per mixing window.
        let mut transcription_dead = false;

        // CRITICAL FIX: Continue processing until channel is closed, not based on recording state
        // This ensures ALL chunks are processed during shutdown, fixing premature meeting completion
        // Previous bug: Loop checked `while self.state.is_recording()` which caused early exit when
        // stop_recording() was called, losing flush signals and remaining chunks in the pipeline
        loop {
            // Receive audio chunks with timeout
            match tokio::time::timeout(
                std::time::Duration::from_millis(50), // Shorter timeout for responsiveness
                self.receiver.recv()
            ).await {
                Ok(Some(chunk)) => {
                    // PERFORMANCE: Check for flush signal (special chunk with ID >= u64::MAX - 10)
                    // Multiple flush signals may be sent to ensure processing
                    if chunk.chunk_id >= u64::MAX - 10 {
                        info!("📥 Received FLUSH signal #{} - flushing VAD processor", u64::MAX - chunk.chunk_id);
                        self.flush_remaining_audio()?;
                        // Continue processing to handle any remaining chunks
                        continue;
                    }

                    // PERFORMANCE OPTIMIZATION: Eliminate per-chunk logging overhead
                    // Logging in hot paths causes severe performance degradation
                    self.processed_chunks += 1;

                    // Smart batching: collect metrics instead of logging every chunk
                    if let Some(ref batcher) = self.metrics_batcher {
                        let avg_level = chunk.data.iter().map(|&x| x.abs()).sum::<f32>() / chunk.data.len() as f32;
                        let duration_ms = chunk.data.len() as f64 / chunk.sample_rate as f64 * 1000.0;

                        batch_audio_metric!(
                            Some(batcher),
                            chunk.chunk_id,
                            chunk.data.len(),
                            duration_ms,
                            avg_level
                        );
                    }

                    // CRITICAL: Log summary only every 200 chunks OR every 60 seconds (99.5% reduction)
                    // This eliminates I/O overhead in the audio processing hot path
                    // Use performance-optimized debug macro that compiles to nothing in release builds
                    if self.processed_chunks % 200 == 0 || self.last_summary_time.elapsed().as_secs() >= 60 {
                        perf_debug!("Pipeline processed {} chunks, current chunk: {} ({} samples)",
                                   self.processed_chunks, chunk.chunk_id, chunk.data.len());
                        self.last_summary_time = std::time::Instant::now();
                    }

                    // STEP 1: Add raw audio to ring buffer for mixing
                    // Microphone audio is already normalized at capture level (AudioCapture)
                    // System audio remains raw
                    self.ring_buffer.add_samples(chunk.device_type.clone(), chunk.data);

                    // STEP 2: Mix audio in fixed windows when both streams have sufficient data
                    while self.ring_buffer.can_mix() {
                        if let Some((mic_window, sys_window)) = self.ring_buffer.extract_window() {
                            // Simple mixing without aggressive ducking
                            let mixed_clean = self.mixer.mix_window(&mic_window, &sys_window);

                            // NO POST-GAIN NEEDED: Microphone already normalized by EBU R128 to -23 LUFS
                            // This is broadcast-standard loudness (Netflix/YouTube/Spotify level)
                            // System audio at natural levels
                            // Previous 2x gain was causing excessive limiting/distortion
                            let mixed_with_gain = mixed_clean;

                            // STEP 3: Send mixed audio for transcription.
                            //
                            // No VAD gating: the transcription engine holds one
                            // continuous stream for the whole meeting and does
                            // its own endpointing, so handing it only speech
                            // segments would break its context across pauses.
                            let samples_16k = super::vad::resample_to_16k(&mixed_with_gain, self.sample_rate);
                            if !samples_16k.is_empty() {
                                let transcription_chunk = AudioChunk {
                                    data: samples_16k,
                                    sample_rate: 16000,
                                    timestamp: self.transcription_samples_sent as f64 / 16000.0,
                                    chunk_id: self.chunk_id_counter,
                                    device_type: DeviceType::Microphone,  // Mixed audio
                                };
                                self.transcription_samples_sent += transcription_chunk.data.len() as u64;

                                if let Err(e) = self.transcription_sender.send(transcription_chunk) {
                                    // The receiver is gone for the rest of the meeting once
                                    // the transcription task exits, and this fires on every
                                    // mixing window — say it once, not twice a second.
                                    if !transcription_dead {
                                        transcription_dead = true;
                                        warn!("Transcription stopped receiving audio ({}); recording continues without a live transcript", e);
                                    }
                                } else {
                                    self.chunk_id_counter += 1;
                                }
                            }

                            // STEP 4: Send mixed audio for recording (WAV file)
                            if let Some(ref sender) = self.recording_sender_for_mixed {
                                let recording_chunk = AudioChunk {
                                    data: mixed_with_gain.clone(),
                                    sample_rate: self.sample_rate,
                                    timestamp: chunk.timestamp,
                                    chunk_id: self.chunk_id_counter,
                                    device_type: DeviceType::Microphone,  // Mixed audio
                                };
                                let _ = sender.send(recording_chunk);
                            }
                        }
                    }
                }
                Ok(None) => {
                    info!("Audio pipeline: sender closed after processing {} chunks", self.processed_chunks);
                    break;
                }
                Err(_) => {
                    // Timeout - just continue, VAD handles all segmentation
                    continue;
                }
            }
        }

        // Flush any remaining VAD segments
        self.flush_remaining_audio()?;

        info!("VAD-driven audio pipeline ended");
        Ok(())
    }

    fn flush_remaining_audio(&mut self) -> Result<()> {
        info!(
            "Flushing remaining audio from pipeline (processed {} chunks, {:.1}s sent for transcription)",
            self.processed_chunks,
            self.transcription_samples_sent as f64 / 16000.0
        );

        // Nothing to flush on the transcription side: audio is forwarded as soon
        // as it is mixed, and the stream's own finalize() drains what the model
        // still holds. Dropping transcription_sender ends the stream loop.
        Ok(())
    }

}

/// Simple audio pipeline manager
pub struct AudioPipelineManager {
    pipeline_handle: Option<JoinHandle<Result<()>>>,
    audio_sender: Option<mpsc::UnboundedSender<AudioChunk>>,
}

impl AudioPipelineManager {
    pub fn new() -> Self {
        Self {
            pipeline_handle: None,
            audio_sender: None,
        }
    }

    /// Start the audio pipeline with device information for adaptive buffering
    pub fn start(
        &mut self,
        state: Arc<RecordingState>,
        transcription_sender: mpsc::UnboundedSender<AudioChunk>,
        target_chunk_duration_ms: u32,
        sample_rate: u32,
        recording_sender: Option<mpsc::UnboundedSender<AudioChunk>>,
        mic_device_name: String,
        mic_device_kind: super::device_detection::InputDeviceKind,
        system_device_name: String,
        system_device_kind: super::device_detection::InputDeviceKind,
    ) -> Result<()> {
        // Log device information for adaptive buffering
        info!("🎙️ Starting pipeline with device info:");
        info!("   Microphone: '{}' ({:?})", mic_device_name, mic_device_kind);
        info!("   System Audio: '{}' ({:?})", system_device_name, system_device_kind);

        // Create audio processing channel
        let (audio_sender, audio_receiver) = mpsc::unbounded_channel::<AudioChunk>();

        // Set sender in state for audio captures to use
        state.set_audio_sender(audio_sender.clone());

        // Create and start pipeline with device information for adaptive mixing
        let mut pipeline = AudioPipeline::new(
            audio_receiver,
            transcription_sender,
            state.clone(),
            target_chunk_duration_ms,
            sample_rate,
            mic_device_name,
            mic_device_kind,
            system_device_name,
            system_device_kind,
        );

        // CRITICAL FIX: Connect recording sender to receive pre-mixed audio
        // This ensures both mic AND system audio are captured in recordings
        pipeline.recording_sender_for_mixed = recording_sender;

        let handle = tokio::spawn(async move {
            pipeline.run().await
        });

        self.pipeline_handle = Some(handle);
        self.audio_sender = Some(audio_sender);

        info!("Audio pipeline manager started with mixed audio recording");
        Ok(())
    }

    /// Stop the audio pipeline
    pub async fn stop(&mut self) -> Result<()> {
        // Drop the sender to close the pipeline
        self.audio_sender = None;

        // Wait for pipeline to finish
        if let Some(handle) = self.pipeline_handle.take() {
            match handle.await {
                Ok(result) => result,
                Err(e) => {
                    error!("Pipeline task failed: {}", e);
                    Ok(())
                }
            }
        } else {
            Ok(())
        }
    }

    /// Force immediate flush of accumulated audio and stop pipeline
    /// PERFORMANCE CRITICAL: Eliminates 30+ second shutdown delays
    pub async fn force_flush_and_stop(&mut self) -> Result<()> {
        info!("🚀 Force flushing pipeline - processing ALL accumulated audio immediately");

        // If we have a sender, send a special flush signal first
        if let Some(sender) = &self.audio_sender {
            // Create a special flush chunk to trigger immediate processing
            let flush_chunk = AudioChunk {
                data: vec![], // Empty data signals flush
                sample_rate: 16000,
                timestamp: 0.0,
                chunk_id: u64::MAX, // Special ID to indicate flush
                device_type: super::recording_state::DeviceType::Microphone,
            };

            if let Err(e) = sender.send(flush_chunk) {
                warn!("Failed to send flush signal: {}", e);
            } else {
                info!("📤 Sent flush signal to pipeline");

                // PERFORMANCE OPTIMIZATION: Reduced wait time from 50ms to 20ms
                // Pipeline should process flush signal very quickly
                tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

                // Send multiple flush signals to ensure the pipeline catches it
                // This aggressive approach eliminates shutdown delay issues
                for i in 0..3 {
                    let additional_flush = AudioChunk {
                        data: vec![],
                        sample_rate: 16000,
                        timestamp: 0.0,
                        chunk_id: u64::MAX - (i as u64),
                        device_type: super::recording_state::DeviceType::Microphone,
                    };
                    let _ = sender.send(additional_flush);
                }

                info!("📤 Sent additional flush signals for reliability");
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }
        }

        // Now stop normally
        self.stop().await
    }
}

impl Default for AudioPipelineManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dismembered-recording bug: system audio delivering faster than the
    /// mic used to emit a window per system-window and zero-fill the mic to
    /// match, so the mic arrived as speech-then-silence and the file grew
    /// longer than the meeting.
    #[test]
    fn live_stream_is_never_padded() {
        let mut rb = AudioMixerRingBuffer::new(48000);
        let w = rb.window_size_samples;

        // Mic honest, system 1.5x too fast.
        for _ in 0..10 {
            rb.add_samples(DeviceType::Microphone, vec![0.5; w / 2]);
            rb.add_samples(DeviceType::System, vec![0.25; w * 3 / 4]);
            while rb.can_mix() {
                rb.extract_window().expect("can_mix said yes");
            }
        }

        assert_eq!(rb.mic_pad, 0, "a live mic must never be zero-padded");
        assert_eq!(rb.windows, 5, "window clock must follow the slower live stream");
    }

    /// ...but a stream that never starts (no system device, denied permission)
    /// must not hold the recording hostage.
    #[test]
    fn absent_stream_does_not_stall_the_mix() {
        let mut rb = AudioMixerRingBuffer::new(48000);
        let w = rb.window_size_samples;

        rb.add_samples(DeviceType::Microphone, vec![0.5; w * 3]);
        while rb.can_mix() {
            rb.extract_window().expect("can_mix said yes");
        }

        assert_eq!(rb.windows, 3);
        assert_eq!(rb.sys_pad, (w * 3) as u64, "missing system audio mixes as silence");
    }
}