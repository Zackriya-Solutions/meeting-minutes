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
use super::vad::{ContinuousVadProcessor};
use super::transcription::provider::STREAM_STEP_SAMPLES;

/// How much silence VAD waits for before deciding an utterance has ended.
///
/// Pauses inside a sentence run roughly 300-700 ms, so anything below that cuts speech
/// mid-phrase: each fragment reaches the model without its context and most of the words
/// are lost. The batch import path already uses 2000 ms for the same reason.
///
/// Raising this delays a segment by the same amount, so it is set to the smallest value
/// measured to keep sentences intact rather than the largest one that works.
const PIPELINE_VAD_REDEMPTION_MS: u32 = 800;

/// Take every complete step out of `accumulator`, leaving the remainder for next time.
///
/// Every returned buffer is exactly [`STREAM_STEP_SAMPLES`] long and what stays behind
/// is always shorter than that. Both halves matter: the encoder advances by one step per
/// call regardless of how much it is handed, so a longer buffer loses the excess with no
/// error, and a shorter one decodes to nothing while its audio waits.
fn take_whole_steps(accumulator: &mut Vec<f32>) -> Vec<Vec<f32>> {
    let mut steps = Vec::new();
    while accumulator.len() >= STREAM_STEP_SAMPLES {
        steps.push(accumulator.drain(..STREAM_STEP_SAMPLES).collect());
    }
    steps
}

/// One audio source on its way to the transcriber.
///
/// Holds everything that must not be shared between sources: a resampler with its own
/// filter state, the audio still waiting to complete a step, and the position on the
/// timeline. Two lanes running side by side is what stops the microphone's room noise
/// from ever reaching the words spoken by people dialling in - summed, the same content
/// measured 62.1% word error rate against 5.9% unmixed.
struct StreamLane {
    source: DeviceType,
    resampler: Option<SincFixedIn<f32>>,
    resampler_chunk: usize,
    pending_input: Vec<f32>,
    /// 16 kHz audio waiting to complete the next step. Never longer than one step.
    accumulator: Vec<f32>,
    /// Absolute 16 kHz sample index of `accumulator[0]`.
    position: usize,
}

impl StreamLane {
    fn new(source: DeviceType, input_sample_rate: u32) -> Result<Self> {
        // Rebuilding a resampler per call loses its filter state at every boundary,
        // which the capture path already learned the hard way.
        let (resampler, resampler_chunk) = if input_sample_rate == 16_000 {
            (None, 0)
        } else {
            let chunk = 1024;
            let parameters = SincInterpolationParameters {
                sinc_len: 256,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 256,
                window: WindowFunction::BlackmanHarris2,
            };
            let built = SincFixedIn::<f32>::new(
                16_000.0 / input_sample_rate as f64,
                2.0,
                parameters,
                chunk,
                1,
            )?;
            (Some(built), chunk)
        };

        Ok(Self {
            source,
            resampler,
            resampler_chunk,
            pending_input: Vec::new(),
            accumulator: Vec::with_capacity(STREAM_STEP_SAMPLES),
            position: 0,
        })
    }

    /// Feed input-rate samples, get back every step they completed.
    ///
    /// Each returned step is paired with the second it starts at, so committed text
    /// lands on the recording's timeline without consulting a voice detector.
    fn push(&mut self, samples: &[f32]) -> Vec<(f64, Vec<f32>)> {
        let resampled = self.to_16k(samples);
        self.accumulator.extend_from_slice(&resampled);

        let mut steps = Vec::new();
        for step in take_whole_steps(&mut self.accumulator) {
            let starts_at = self.position as f64 / 16_000.0;
            self.position += step.len();
            steps.push((starts_at, step));
        }
        steps
    }

    /// Pad the leftover into one final step so the end of a recording is not lost.
    ///
    /// A step must be exactly one step long, so without this the last few hundred
    /// milliseconds would never be decoded - and that is where Stop lands, which is
    /// exactly when the speaker was mid-word.
    fn flush(&mut self) -> Option<(f64, Vec<f32>)> {
        if self.accumulator.is_empty() {
            return None;
        }
        let mut tail = std::mem::take(&mut self.accumulator);
        let starts_at = self.position as f64 / 16_000.0;
        self.position += tail.len();
        tail.resize(STREAM_STEP_SAMPLES, 0.0);
        Some((starts_at, tail))
    }

    fn to_16k(&mut self, samples: &[f32]) -> Vec<f32> {
        let chunk = self.resampler_chunk;
        let source = self.source.clone();
        let resampler = match self.resampler.as_mut() {
            Some(resampler) => resampler,
            None => return samples.to_vec(),
        };

        self.pending_input.extend_from_slice(samples);
        let mut out = Vec::new();
        while self.pending_input.len() >= chunk {
            let input: Vec<f32> = self.pending_input.drain(..chunk).collect();
            match resampler.process(&[input], None) {
                Ok(mut waves) if !waves.is_empty() => out.append(&mut waves[0]),
                Ok(_) => {}
                Err(e) => {
                    warn!("resampling {:?} failed: {}", source, e);
                    break;
                }
            }
        }
        out
    }
}

/// Ring buffer for synchronized audio mixing
/// Accumulates samples from mic and system streams until we have aligned windows
struct AudioMixerRingBuffer {
    mic_buffer: VecDeque<f32>,
    system_buffer: VecDeque<f32>,
    window_size_samples: usize,  // Fixed mixing window (e.g., 50ms)
    max_buffer_size: usize,  // Safety limit (e.g., 100ms)
}

impl AudioMixerRingBuffer {
    fn new(sample_rate: u32) -> Self {
        // VAD only ever sees audio one window at a time, so the window is also
        // the resolution at which speech onsets can be noticed. At the 600 ms
        // this used to be, the first words of a fast utterance were routinely
        // rounded away; 100 ms still amortises the mixing work while letting the
        // detector react six times sooner.
        let window_ms = 100.0;
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
        }
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
            DeviceType::Microphone => self.mic_buffer.extend(samples),
            DeviceType::System => self.system_buffer.extend(samples),
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

    fn can_mix(&self) -> bool {
        self.mic_buffer.len() >= self.window_size_samples ||
        self.system_buffer.len() >= self.window_size_samples
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
            let mut padded = Vec::with_capacity(self.window_size_samples);
            padded.extend_from_slice(&available);

            // Use zero-padding (silence) to prevent repetition artifacts
            // Zero-padding is inaudible at 48kHz sample rate
            padded.resize(self.window_size_samples, 0.0);

            padded
        } else {
            // No mic data - return silence
            vec![0.0; self.window_size_samples]
        };

        // Extract system window (or pad with zeros if insufficient data)
        let sys_window = if self.system_buffer.len() >= self.window_size_samples {
            // Enough system data - drain window
            self.system_buffer.drain(0..self.window_size_samples).collect()
        } else if !self.system_buffer.is_empty() {
            // Some system data but not enough - consume all + pad with zeros
            let available: Vec<f32> = self.system_buffer.drain(..).collect();
            let mut padded = Vec::with_capacity(self.window_size_samples);
            padded.extend_from_slice(&available);

            // Use zero-padding (silence) to prevent repetition artifacts
            // Zero-padding is inaudible at 48kHz sample rate
            padded.resize(self.window_size_samples, 0.0);

            padded
        } else {
            // No system data - return silence
            vec![0.0; self.window_size_samples]
        };

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

            // Adaptive parameters based on sample rate ratio (same logic as resample_audio)
            let (sinc_len, interpolation_type, oversampling) = if ratio >= 2.0 {
                (512, SincInterpolationType::Cubic, 512)
            } else if ratio >= 1.5 {
                (384, SincInterpolationType::Cubic, 384)
            } else if ratio > 1.0 {
                (256, SincInterpolationType::Linear, 256)
            } else if ratio <= 0.5 {
                (512, SincInterpolationType::Cubic, 512)
            } else {
                (384, SincInterpolationType::Linear, 384)
            };

            let params = SincInterpolationParameters {
                sinc_len,
                f_cutoff: 0.95,
                interpolation: interpolation_type,
                oversampling_factor: oversampling,
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
        }
    }

    /// Process audio data directly from callback
    pub fn process_audio_data(&self, data: &[f32]) {
        // Check if still recording
        if !self.state.is_recording() {
            return;
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
        let audio_chunk = AudioChunk::final_chunk(
            mono_data,  // Raw audio (resampled if needed), no gain yet
            if self.needs_resampling { 48000 } else { self.sample_rate },
            timestamp,
            chunk_id,
            self.device_type.clone(),
        );

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
    vad_processor: ContinuousVadProcessor,
    sample_rate: u32,
    chunk_id_counter: u64,
    /// Identifies the utterance currently being spoken, so the partials sent while it is
    /// still going and the final segment that closes it share one identity.
    utterance_id_counter: u64,
    /// Speech accumulated since the current utterance began, resent in full on every
    /// partial. Resending the whole thing rather than only the new audio is what keeps
    /// partial text as accurate as the final - the model never sees a clipped phrase.
    partial_speech: Vec<f32>,
    /// Length `partial_speech` had reached when the last partial went out.
    last_partial_len: usize,
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
    /// The two transcription lanes. The mixer still feeds the recorder, but the
    /// transcriber never sees the blend: summing the microphone into the system audio
    /// adds a delayed room copy of the same words, which measured 62.1% word error
    /// rate against 5.9% for the same content unmixed.
    mic_lane: StreamLane,
    system_lane: StreamLane,
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

        // The VAD processor handles 48kHz->16kHz resampling internally.
        let vad_processor = match ContinuousVadProcessor::new(sample_rate, PIPELINE_VAD_REDEMPTION_MS) {
            Ok(processor) => {
                info!("VAD-driven pipeline: VAD segments will be sent directly to Whisper (no time-based accumulation)");
                processor
            }
            Err(e) => {
                error!("Failed to create VAD processor: {}", e);
                panic!("VAD processor creation failed: {}", e);
            }
        };

        // Initialize professional audio mixing components
        let ring_buffer = AudioMixerRingBuffer::new(sample_rate);
        let mixer = ProfessionalAudioMixer::new(sample_rate);

        // Note: target_chunk_duration_ms is ignored - VAD controls segmentation now
        let _ = target_chunk_duration_ms;

        Self {
            receiver,
            transcription_sender,
            state,
            vad_processor,
            sample_rate,
            chunk_id_counter: 0,
            utterance_id_counter: 0,
            partial_speech: Vec::new(),
            last_partial_len: 0,
            // Performance optimization: reduce logging frequency
            last_summary_time: std::time::Instant::now(),
            processed_chunks: 0,
            // Initialize metrics batcher for smart batching
            metrics_batcher: Some(AudioMetricsBatcher::new()),
            // Initialize professional audio mixing
            ring_buffer,
            mixer,
            recording_sender_for_mixed: None,  // Will be set by manager
            mic_lane: StreamLane::new(DeviceType::Microphone, sample_rate)
                .expect("microphone lane"),
            system_lane: StreamLane::new(DeviceType::System, sample_rate)
                .expect("system lane"),
        }
    }

    /// Hand every completed 560 ms step of the mixed stream to the transcriber.
    ///
    /// This runs unconditionally, alongside VAD rather than after it, because the
    /// pipeline starts before the engine has finished loading and so cannot know
    /// whether a streaming engine is on the other end. A worker driving Whisper
    /// discards these; a worker driving Nemotron discards the VAD segments instead.
    ///
    /// Sending audio VAD rejected is the entire point. VAD forwarded about two
    /// thirds of it, and spans shorter than one step came back empty, which together
    /// account for the missing words in the live transcript.
    fn dispatch_stream_steps(&mut self, mic_window: &[f32], sys_window: &[f32]) {
        let batches = [
            (DeviceType::Microphone, self.mic_lane.push(mic_window)),
            (DeviceType::System, self.system_lane.push(sys_window)),
        ];

        for (source, steps) in batches {
            for (starts_at, step) in steps {
                let chunk = AudioChunk::stream_step(
                    step,
                    16_000,
                    starts_at,
                    self.chunk_id_counter,
                    source.clone(),
                );
                self.chunk_id_counter += 1;

                if let Err(e) = self.transcription_sender.send(chunk) {
                    warn!("Failed to send streaming step: {}", e);
                    return;
                }
            }
        }
    }

    /// Send the utterance so far while the speaker is still talking, so text appears
    /// before they pause instead of only after.
    ///
    /// The whole utterance is resent each time rather than just the new audio. Sending
    /// only the new slice would hand the model a phrase clipped at an arbitrary point,
    /// which measurably wrecks the transcript - a 6 s slice of ordinary speech came back
    /// as the single word "amazing". Re-running the full utterance costs GPU time but
    /// keeps every partial as accurate as the final.
    fn emit_partial_if_due(&mut self) {
        /// Utterances shorter than this finish on their own quickly enough that a partial
        /// would only duplicate work.
        const MIN_UTTERANCE_SAMPLES: usize = 3 * 16_000 / 2; // 1.5 s
        /// How much new speech has to arrive before the next partial.
        const PARTIAL_STRIDE_SAMPLES: usize = 2 * 16_000; // 2 s
        /// Point past which previewing stops.
        ///
        /// Each partial re-runs the whole utterance, so the work done across one
        /// utterance grows with the square of its length. By this point there is already
        /// plenty of text on screen, and continuing would put long transcriptions in the
        /// queue ahead of the final segment - delaying the very text that gets kept.
        const MAX_PREVIEWED_SAMPLES: usize = 15 * 16_000; // 15 s

        // A streaming engine already paints text every 560 ms from the other lane, and
        // the worker throws these away. Re-decoding the whole utterance to produce them
        // would be the most expensive thing the pipeline does for nobody.
        if crate::audio::transcription::worker::STREAMING_ACTIVE
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return;
        }

        let in_progress = match self.vad_processor.speech_in_progress() {
            Some(samples) => samples,
            None => return,
        };

        if in_progress.len() < MIN_UTTERANCE_SAMPLES
            || in_progress.len() > MAX_PREVIEWED_SAMPLES
            || in_progress.len() < self.last_partial_len + PARTIAL_STRIDE_SAMPLES
        {
            return;
        }

        self.partial_speech.clear();
        self.partial_speech.extend_from_slice(in_progress);
        self.last_partial_len = self.partial_speech.len();

        info!(
            "📝 Partial for utterance {}: {:.1}s so far",
            self.utterance_id_counter,
            self.partial_speech.len() as f64 / 16000.0
        );

        let partial = AudioChunk {
            data: self.partial_speech.clone(),
            sample_rate: 16000,
            timestamp: 0.0,
            chunk_id: self.chunk_id_counter,
            device_type: DeviceType::Microphone,
            is_partial: true,
            utterance_id: Some(self.utterance_id_counter),
            is_stream_step: false,
        };

        if let Err(e) = self.transcription_sender.send(partial) {
            warn!("Failed to send partial: {}", e);
        }
    }

    /// Run the VAD-driven audio processing pipeline
    pub async fn run(mut self) -> Result<()> {
        info!("VAD-driven audio pipeline started - segments sent in real-time based on speech detection");

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

                            // STEP 3: Send mixed audio for transcription (VAD + Whisper)
                            match self.vad_processor.process_audio(&mixed_with_gain) {
                                Ok(speech_segments) => {
                                    for segment in speech_segments {
                                        let duration_ms = segment.end_timestamp_ms - segment.start_timestamp_ms;

                                        if segment.samples.len() >= 800 {  // Minimum 50ms at 16kHz - matches Parakeet capability
                                            info!("📤 Sending VAD segment: {:.1}ms, {} samples",
                                                  duration_ms, segment.samples.len());

                                            let transcription_chunk = AudioChunk {
                                                data: segment.samples,
                                                sample_rate: 16000,
                                                timestamp: segment.start_timestamp_ms / 1000.0,
                                                chunk_id: self.chunk_id_counter,
                                                device_type: DeviceType::Microphone,  // Mixed audio
                                                is_partial: false,
                                                utterance_id: Some(self.utterance_id_counter),
                                                is_stream_step: false,
                                            };

                                            if let Err(e) = self.transcription_sender.send(transcription_chunk) {
                                                warn!("Failed to send VAD segment: {}", e);
                                            } else {
                                                self.chunk_id_counter += 1;
                                            }
                                        } else {
                                            debug!("⏭️ Dropping short VAD segment: {:.1}ms ({} samples < 800)",
                                                   duration_ms, segment.samples.len());
                                        }

                                        // The utterance is closed either way; the next one
                                        // gets a fresh identity and partial buffer.
                                        self.utterance_id_counter += 1;
                                        self.partial_speech.clear();
                                        self.last_partial_len = 0;
                                    }

                                    self.emit_partial_if_due();
                                }
                                Err(e) => {
                                    // Fall through to dispatch_stream_steps below: a
                                    // detector failure must not stop the stream, which
                                    // does not depend on VAD's verdict at all.
                                    // The processor keeps unflagged audio in its
                                    // ledger, so a failure here delays the span
                                    // rather than destroying it.
                                    warn!("⚠️ VAD error (audio retained for recovery): {}", e);
                                }
                            }

                            // STEP 3b: hand each source to its own streaming lane -
                            // whole, unfiltered, and crucially unmixed.
                            self.dispatch_stream_steps(&mic_window, &sys_window);

                            // STEP 4: Send mixed audio for recording (WAV file)
                            if let Some(ref sender) = self.recording_sender_for_mixed {
                                let recording_chunk = AudioChunk::final_chunk(
                                    mixed_with_gain.clone(),
                                    self.sample_rate,
                                    chunk.timestamp,
                                    self.chunk_id_counter,
                                    DeviceType::Microphone,  // Mixed audio
                                );
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
        info!("Flushing remaining audio from pipeline (processed {} chunks)", self.processed_chunks);

        // The streaming lanes first, padding each tail rather than dropping it. A step
        // must be exactly one step long, so the last few hundred milliseconds of a
        // recording would otherwise never be decoded - and that is where "stop" lands,
        // which is exactly when the speaker was mid-word.
        let tails = [
            (DeviceType::Microphone, self.mic_lane.flush()),
            (DeviceType::System, self.system_lane.flush()),
        ];
        for (source, tail) in tails {
            if let Some((starts_at, samples)) = tail {
                let chunk = AudioChunk::stream_step(
                    samples,
                    16_000,
                    starts_at,
                    self.chunk_id_counter,
                    source,
                );
                self.chunk_id_counter += 1;
                if let Err(e) = self.transcription_sender.send(chunk) {
                    warn!("Failed to send the final streaming step: {}", e);
                }
            }
        }

        // Flush any remaining audio from VAD processor and send segments to transcription
        match self.vad_processor.flush() {
            Ok(final_segments) => {
                for segment in final_segments {
                    let duration_ms = segment.end_timestamp_ms - segment.start_timestamp_ms;

                    // Send segments >= 50ms (800 samples at 16kHz) - matches main pipeline filter
                    if segment.samples.len() >= 800 {
                        info!("📤 Sending final VAD segment to Whisper: {:.1}ms duration, {} samples",
                              duration_ms, segment.samples.len());

                        let transcription_chunk = AudioChunk {
                            data: segment.samples,
                            sample_rate: 16000,
                            timestamp: segment.start_timestamp_ms / 1000.0,
                            chunk_id: self.chunk_id_counter,
                            device_type: DeviceType::Microphone,
                            is_partial: false,
                            utterance_id: Some(self.utterance_id_counter),
                            is_stream_step: false,
                        };

                        if let Err(e) = self.transcription_sender.send(transcription_chunk) {
                            warn!("Failed to send final VAD segment: {}", e);
                        } else {
                            self.chunk_id_counter += 1;
                        }
                        self.utterance_id_counter += 1;
                    } else {
                        info!("⏭️ Skipping short final segment: {:.1}ms ({} samples < 800)",
                              duration_ms, segment.samples.len());
                    }
                }
            }
            Err(e) => {
                warn!("Failed to flush VAD processor: {}", e);
            }
        }

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
            let flush_chunk = AudioChunk::final_chunk(
                vec![], // Empty data signals flush
                16000,
                0.0,
                u64::MAX, // Special ID to indicate flush
                super::recording_state::DeviceType::Microphone,
            );

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
                    let additional_flush = AudioChunk::final_chunk(
                        vec![],
                        16000,
                        0.0,
                        u64::MAX - (i as u64),
                        super::recording_state::DeviceType::Microphone,
                    );
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

    /// Guards against the redemption time drifting back down.
    ///
    /// Measured on a 42 s recording of ordinary speech, transcribed segment by segment
    /// with Parakeet (see `vad_redemption_time_changes_transcript_quality`):
    ///
    ///   400 ms -> 5 segments, mean 1486 ms: "Mm-hmm. | I mean | Then | Maybe the"
    ///   800 ms -> 1 segment,  mean 8620 ms: "But this is just millions of dollars per
    ///                                        minute. Maybe they'll make you"
    ///
    /// Natural pauses inside a sentence run 300-700 ms, so a 400 ms threshold decides the
    /// utterance ended mid-phrase and most of the words never reach the model.
    #[test]
    fn vad_redemption_bridges_pauses_inside_a_sentence() {
        assert!(
            PIPELINE_VAD_REDEMPTION_MS >= 800,
            "redemption of {}ms cuts speech at pauses inside a sentence",
            PIPELINE_VAD_REDEMPTION_MS
        );
    }

    /// A step is 560 ms at 16 kHz because that is what the encoder consumes per call.
    #[test]
    fn a_stream_step_is_560ms_at_16khz() {
        assert_eq!(STREAM_STEP_SAMPLES, 560 * 16_000 / 1000);
    }

    /// The invariant that separates 5.9% word error rate from 62.6%.
    ///
    /// Handing the encoder more than one step per call does not fail: it decodes the
    /// first step, buffers the rest and never catches up. So the slicing has to be exact
    /// in both directions - every dispatched buffer a whole step, every partial step held
    /// back rather than sent short.
    #[test]
    fn steps_are_taken_whole_and_the_remainder_is_kept() {
        let mut accumulator: Vec<f32> = (0..STREAM_STEP_SAMPLES * 2 + 100)
            .map(|i| i as f32)
            .collect();

        let steps = take_whole_steps(&mut accumulator);

        assert_eq!(steps.len(), 2, "two whole steps were available");
        assert!(
            steps.iter().all(|s| s.len() == STREAM_STEP_SAMPLES),
            "a step must never be sent short or long"
        );
        assert_eq!(accumulator.len(), 100, "the partial step waits for more audio");

        // Contiguous and in order: a gap here would be lost audio, an overlap would be
        // words decoded twice.
        assert_eq!(steps[0][0], 0.0);
        assert_eq!(steps[1][0], STREAM_STEP_SAMPLES as f32);
        assert_eq!(accumulator[0], (STREAM_STEP_SAMPLES * 2) as f32);
    }

    /// Each source keeps its own place on the timeline. Sharing a counter would put
    /// one stream's text at the other stream's timestamps.
    #[test]
    fn lanes_advance_independently() {
        let mut mic = StreamLane::new(DeviceType::Microphone, 16_000).expect("lane");
        let mut system = StreamLane::new(DeviceType::System, 16_000).expect("lane");

        let one_step = vec![0.0f32; STREAM_STEP_SAMPLES];
        let mic_steps = mic.push(&one_step);
        assert_eq!(mic_steps.len(), 1);
        assert_eq!(mic_steps[0].0, 0.0, "first mic step starts at zero");

        let two_steps = [one_step.clone(), one_step.clone()].concat();
        let system_steps = system.push(&two_steps);
        assert_eq!(system_steps.len(), 2);
        assert_eq!(system_steps[1].0, 0.56, "second system step starts at 560 ms");

        // The microphone lane is untouched by the system lane's two steps.
        let next_mic = mic.push(&one_step);
        assert_eq!(next_mic[0].0, 0.56, "mic resumed from its own position");
    }

    /// A lane holds a partial step rather than sending it short, and pads it on flush.
    #[test]
    fn a_lane_holds_a_partial_step_until_flush() {
        let mut lane = StreamLane::new(DeviceType::System, 16_000).expect("lane");
        assert!(lane.push(&vec![0.0f32; STREAM_STEP_SAMPLES - 1]).is_empty());

        let (starts_at, tail) = lane.flush().expect("the tail must not be dropped");
        assert_eq!(starts_at, 0.0);
        assert_eq!(
            tail.len(),
            STREAM_STEP_SAMPLES,
            "the tail is padded, never sent short"
        );
    }

    /// Nothing is emitted until a whole step exists, and nothing is consumed either.
    #[test]
    fn a_partial_step_is_never_dispatched() {
        let mut accumulator: Vec<f32> = vec![1.0; STREAM_STEP_SAMPLES - 1];
        assert!(take_whole_steps(&mut accumulator).is_empty());
        assert_eq!(accumulator.len(), STREAM_STEP_SAMPLES - 1);
    }

    /// The whole point of separating the lanes, stated as a number.
    ///
    /// Runs both lanes at once: the system lane carries the clean source, the
    /// microphone lane carries the same content after it went out of a speaker and back
    /// in through a microphone. Summed, those two scored 62.1% word error rate. Kept
    /// apart, the system lane must be unaffected by its noisy neighbour.
    ///
    ///   $env:MEETILY_STREAM_CASE="tests/fixtures/watchshop_60s_16k.f32"
    ///   $env:MEETILY_STREAM_NOISY="tests/fixtures/device_60s_16k.f32"
    ///   $env:MEETILY_STREAM_REFERENCE="tests/fixtures/watchshop_60s_reference.txt"
    ///   cargo test --features cuda --lib separates -- --ignored --nocapture
    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "needs MEETILY_STREAM_CASE, MEETILY_STREAM_NOISY and the Nemotron model"]
    async fn separates_a_clean_source_from_a_noisy_one() {
        use crate::audio::transcription::nemotron_provider::{
            NemotronProvider, DEFAULT_NEMOTRON_MODEL,
        };
        use crate::audio::transcription::provider::TranscriptionProvider;
        use std::path::PathBuf;

        fn load(key: &str) -> Vec<f32> {
            let path = std::env::var(key).unwrap_or_else(|_| panic!("set {key}"));
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
            bytes
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect()
        }

        let clean = load("MEETILY_STREAM_CASE");
        let noisy = load("MEETILY_STREAM_NOISY");

        std::env::set_var(
            "MEETILY_NEMOTRON_HELPER",
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("binaries")
                .join("nemotron-helper-x86_64-pc-windows-msvc.exe"),
        );
        let model_dir = PathBuf::from(std::env::var("APPDATA").unwrap())
            .join("com.meetily.ai")
            .join("models")
            .join("nemotron")
            .join(DEFAULT_NEMOTRON_MODEL);
        let provider = NemotronProvider::new(model_dir, Some("en-US".to_string()));
        provider.ensure_started().await.expect("sidecar should start");

        // Both fixtures are already 16 kHz, so the lanes pass audio straight through.
        let mut system_lane = StreamLane::new(DeviceType::System, 16_000).expect("lane");
        let mut mic_lane = StreamLane::new(DeviceType::Microphone, 16_000).expect("lane");
        let mut system_text = String::new();

        // Interleaved in 100 ms windows, the way the mixer hands them over live.
        let shared = clean.len().min(noisy.len());
        for start in (0..shared).step_by(1_600) {
            let end = (start + 1_600).min(shared);

            for (_, step) in mic_lane.push(&noisy[start..end]) {
                provider
                    .transcribe_step(step, DeviceType::Microphone)
                    .await
                    .expect("mic step");
            }
            for (_, step) in system_lane.push(&clean[start..end]) {
                let piece = provider
                    .transcribe_step(step, DeviceType::System)
                    .await
                    .expect("system step");
                system_text.push_str(&piece);
            }
        }

        assert_eq!(
            provider.live_stream_count().await,
            2,
            "each lane must have brought up its own decoder"
        );
        println!("system stream: {}", system_text.trim());

        let reference = std::fs::read_to_string(
            std::env::var("MEETILY_STREAM_REFERENCE").expect("set MEETILY_STREAM_REFERENCE"),
        )
        .expect("reference");
        let wer = word_error_rate(&reference, &system_text);
        println!("system stream wer {:.1}%", wer * 100.0);

        // Mixed, this exact pairing scored 62.1%. Unmixed, the clean lane must land near
        // its own ceiling; 10% leaves room for variation without tolerating the
        // contamination this change removes.
        assert!(
            wer <= 0.10,
            "the noisy microphone contaminated the system stream: {:.1}%",
            wer * 100.0
        );
    }

    /// Word error rate against a reference, on normalised words.
    ///
    /// Folds case, punctuation, digits-versus-words and casual spellings, matching
    /// `tests/streaming_bench.py` so the two harnesses produce comparable numbers.
    /// None of those differences are mishearings, and charging for them would mean
    /// tuning the transcript's formatter instead of its accuracy.
    #[cfg(windows)]
    fn word_error_rate(reference: &str, hypothesis: &str) -> f64 {
        fn words(text: &str) -> Vec<String> {
            const SPELLED: [&str; 20] = [
                "zero", "one", "two", "three", "four", "five", "six", "seven", "eight",
                "nine", "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen",
                "sixteen", "seventeen", "eighteen", "nineteen",
            ];
            const COLLOQUIAL: [(&str, &str); 9] = [
                ("gonna", "going to"),
                ("wanna", "want to"),
                ("gotta", "got to"),
                ("kinda", "kind of"),
                ("sorta", "sort of"),
                ("cuz", "because"),
                ("yep", "yeah"),
                ("yup", "yeah"),
                ("ok", "okay"),
            ];

            text.to_lowercase()
                .replace(">>", " ")
                .replace('-', " ")
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '\'')
                .filter(|w| !w.is_empty())
                .flat_map(|w| {
                    let expanded = match COLLOQUIAL.iter().find(|(from, _)| *from == w) {
                        Some((_, to)) => to.to_string(),
                        None => match w.parse::<usize>() {
                            Ok(n) if n < SPELLED.len() => SPELLED[n].to_string(),
                            _ => w.to_string(),
                        },
                    };
                    expanded
                        .split(' ')
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .collect()
        }

        let (reference, hypothesis) = (words(reference), words(hypothesis));
        if reference.is_empty() {
            return f64::NAN;
        }

        let mut row: Vec<usize> = (0..=hypothesis.len()).collect();
        for (i, r) in reference.iter().enumerate() {
            let mut next = vec![i + 1];
            for (j, h) in hypothesis.iter().enumerate() {
                let cost = if r == h { 0 } else { 1 };
                next.push(
                    (row[j] + cost)
                        .min(row[j + 1] + 1)
                        .min(next[j] + 1),
                );
            }
            row = next;
        }

        row[hypothesis.len()] as f64 / reference.len() as f64
    }

    /// End-to-end offline check of the streaming lane, through the real components.
    ///
    /// `tests/streaming_bench.py` proves the *model* can stream; this proves the
    /// *pipeline* feeds it correctly. It walks the same path a recording does - the VAD
    /// processor's 48 kHz resampler, the step accumulator, the provider - and differs
    /// only in that the audio comes from a file instead of a device. That makes it the
    /// thing to run after touching any of them, instead of launching the app.
    ///
    /// Needs the Nemotron model and the staged sidecar, so it is ignored by default:
    ///
    ///   $env:MEETILY_STREAM_CASE="tests/fixtures/watchshop_60s_48k.f32"
    ///   $env:MEETILY_STREAM_REFERENCE="tests/fixtures/watchshop_60s_reference.txt"
    ///   cargo test --features cuda --lib streams_a_real_recording -- --ignored --nocapture
    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "needs MEETILY_STREAM_CASE plus the Nemotron model"]
    async fn streams_a_real_recording_without_losing_words() {
        use crate::audio::transcription::nemotron_provider::{
            NemotronProvider, DEFAULT_NEMOTRON_MODEL,
        };
        use crate::audio::transcription::provider::TranscriptionProvider;
        use std::path::PathBuf;

        let case = std::env::var("MEETILY_STREAM_CASE")
            .expect("set MEETILY_STREAM_CASE to a raw f32le 48 kHz mono file");
        let bytes = std::fs::read(&case).unwrap_or_else(|e| panic!("{case}: {e}"));
        let samples: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        let audio_seconds = samples.len() as f64 / 48_000.0;

        std::env::set_var(
            "MEETILY_NEMOTRON_HELPER",
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("binaries")
                .join("nemotron-helper-x86_64-pc-windows-msvc.exe"),
        );
        let model_dir = PathBuf::from(std::env::var("APPDATA").unwrap())
            .join("com.meetily.ai")
            .join("models")
            .join("nemotron")
            .join(DEFAULT_NEMOTRON_MODEL);
        // Named, not "auto", because that is what ships: engine.rs pins the language so
        // the model is not left guessing it. Measuring "auto" here would score a
        // configuration nobody runs.
        let provider = NemotronProvider::new(model_dir, Some("en-US".to_string()));
        provider.ensure_started().await.expect("sidecar should start");

        // One lane, fed the whole clip: this case has a single source, so it exercises
        // the same path a system-audio-only meeting takes.
        let mut lane = StreamLane::new(DeviceType::System, 48_000).expect("lane");

        let mut transcript = String::new();
        let mut first_word_at: Option<f64> = None;
        let mut last_word_at = 0.0f64;
        let mut worst_gap = 0.0f64;
        // Where the worst gap starts, so a long one can be listened to rather than
        // guessed at - silence in the recording and the model going quiet look
        // identical in the number alone.
        let mut worst_gap_at = 0.0f64;
        // Every gap, because the maximum alone cannot tell them apart. Over ten minutes
        // of this recording the median gap is 0.56 s - one step - while three isolated
        // gaps run past 3 s at the video's title cards. Judging the stream by its worst
        // moment would fail on a meeting that simply had a quiet minute.
        let mut gaps: Vec<f64> = Vec::new();
        let began = std::time::Instant::now();

        // 100 ms windows, the size the mixer hands over during a real recording.
        for window in samples.chunks(4_800) {
            for (starts_at, step) in lane.push(window) {
                let at = starts_at + step.len() as f64 / 16_000.0;

                let piece = provider
                    .transcribe_step(step, DeviceType::System)
                    .await
                    .expect("a whole step must be accepted");

                if !piece.trim().is_empty() {
                    transcript.push_str(&piece);
                    first_word_at.get_or_insert(at);
                    gaps.push(at - last_word_at);
                    if at - last_word_at > worst_gap {
                        worst_gap = at - last_word_at;
                        worst_gap_at = last_word_at;
                    }
                    last_word_at = at;
                }
            }
        }
        gaps.push(audio_seconds - last_word_at);
        if audio_seconds - last_word_at > worst_gap {
            worst_gap = audio_seconds - last_word_at;
            worst_gap_at = last_word_at;
        }

        let compute_seconds = began.elapsed().as_secs_f64();
        let rtf = compute_seconds / audio_seconds;
        let first = first_word_at.expect("the stream committed no text at all");

        gaps.sort_by(|a, b| a.partial_cmp(b).expect("gaps are finite"));
        let median_gap = gaps[gaps.len() / 2];
        let p95_gap = gaps[gaps.len() * 95 / 100];

        println!("transcript: {}", transcript.trim());
        println!(
            "audio {audio_seconds:.1}s  rtf {rtf:.2}  first word {first:.1}s  \
             gap median {median_gap:.2}s p95 {p95_gap:.2}s  \
             worst {worst_gap:.1}s starting at {worst_gap_at:.1}s"
        );

        // Thresholds from the design document. They are deliberately a little looser
        // than the measured values so ordinary variation does not fail the build, but
        // tight enough that the failure this change fixes cannot come back unnoticed:
        // before it, the first commit landed after Stop and the worst gap was the whole
        // recording.
        assert!(first <= 2.0, "first committed word took {first:.1}s");
        // The distribution, not the maximum: a recording is allowed to contain silence,
        // and one quiet stretch must not fail a stream that is otherwise committing
        // every step. Measured p95 is 1.12 s over ten minutes, so 2.0 s leaves room
        // without tolerating the failure this change fixes - before it, *every* gap was
        // the length of the recording.
        assert!(p95_gap <= 2.0, "95% of gaps reached {p95_gap:.2}s between commits");
        assert!(rtf <= 0.5, "real-time factor {rtf:.2} leaves no headroom");

        if let Ok(path) = std::env::var("MEETILY_STREAM_REFERENCE") {
            let reference = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
            let wer = word_error_rate(&reference, &transcript);
            println!("wer {:.1}%", wer * 100.0);
            // Set to catch structural breakage - a lane wired wrong, steps sized wrong,
            // pieces joined wrong - all of which land above 60%. It is deliberately not
            // tight enough to police single words: 203 reference words make one
            // substitution worth 0.5%, so a gate near the measured value would fail on
            // noise rather than on regressions. Measured: 5.9% with the language left to
            // the model, 6.4% with en-US pinned.
            assert!(wer <= 0.10, "word error rate {:.1}%", wer * 100.0);
        }
    }
}