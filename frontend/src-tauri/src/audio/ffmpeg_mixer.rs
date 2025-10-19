// FFmpeg-Style Adaptive Audio Mixer
//
// This mixer implements Cap's adaptive buffering strategy with per-source buffering,
// gap detection, and device-aware timeout management for optimal Bluetooth support.
//
// Key Features:
// 1. Per-source buffering (not shared ring buffer)
// 2. Adaptive timeouts based on device type (Wired: 20-50ms, Bluetooth: 80-200ms)
// 3. Gap detection and silence insertion for Bluetooth jitter
// 4. Timestamp-aware mixing to maintain sync
// 5. Professional audio mixing with RMS-based ducking

use std::collections::VecDeque;
use std::time::{Duration, Instant};
use log::{debug, warn, info};

use super::device_detection::InputDeviceKind;

/// Configuration flags for audio processing features
pub const RNNOISE_APPLY_ENABLED: bool = false;  // Default: disabled (Whisper handles noise well)

/// Timestamp for audio samples (reserved for future use)
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
struct Timestamp {
    instant: Instant,
    sample_count: u64,
}

#[allow(dead_code)]
impl Timestamp {
    fn new() -> Self {
        Self {
            instant: Instant::now(),
            sample_count: 0,
        }
    }

    fn advance(&mut self, samples: usize) {
        self.sample_count += samples as u64;
    }

    fn elapsed(&self) -> Duration {
        self.instant.elapsed()
    }
}

/// Audio chunk with timestamp information
#[derive(Debug, Clone)]
struct TimestampedChunk {
    samples: Vec<f32>,
    timestamp: Instant,
    #[allow(dead_code)]
    sample_rate: u32,
}

impl TimestampedChunk {
    fn new(samples: Vec<f32>, sample_rate: u32) -> Self {
        Self {
            samples,
            timestamp: Instant::now(),
            sample_rate,
        }
    }

    /// Calculate the duration of this chunk in milliseconds (reserved for future use)
    #[allow(dead_code)]
    fn duration_ms(&self) -> f64 {
        (self.samples.len() as f64 / self.sample_rate as f64) * 1000.0
    }

    /// Calculate the age of this chunk (time since capture)
    fn age(&self) -> Duration {
        self.timestamp.elapsed()
    }
}

/// Per-source audio buffer with adaptive timeout
struct SourceBuffer {
    /// Device name for logging
    device_name: String,

    /// Detected device type (Wired/Bluetooth/Unknown)
    device_kind: InputDeviceKind,

    /// Buffered audio chunks with timestamps
    chunks: VecDeque<TimestampedChunk>,

    /// Adaptive buffer timeout (based on device type)
    buffer_timeout: Duration,

    /// Sample rate for this source
    sample_rate: u32,

    /// Total samples buffered
    total_samples: usize,

    /// Statistics
    chunks_received: u64,
    gaps_detected: u32,
    silence_inserted_samples: u64,
    last_chunk_time: Option<Instant>,
}

impl SourceBuffer {
    fn new(device_name: String, device_kind: InputDeviceKind, sample_rate: u32) -> Self {
        // Get adaptive timeout based on device type
        let (min_timeout, max_timeout) = device_kind.buffer_timeout();

        // Use max timeout for initial conservative approach
        let buffer_timeout = max_timeout;

        info!("📦 SourceBuffer created for '{}' ({:?})", device_name, device_kind);
        info!("   Sample rate: {} Hz", sample_rate);
        info!("   Buffer timeout: {:.0}ms (range: {:.0}ms - {:.0}ms)",
              buffer_timeout.as_secs_f64() * 1000.0,
              min_timeout.as_secs_f64() * 1000.0,
              max_timeout.as_secs_f64() * 1000.0);

        Self {
            device_name,
            device_kind,
            chunks: VecDeque::new(),
            buffer_timeout,
            sample_rate,
            total_samples: 0,
            chunks_received: 0,
            gaps_detected: 0,
            silence_inserted_samples: 0,
            last_chunk_time: None,
        }
    }

    /// Push new audio chunk to the buffer
    fn push(&mut self, samples: Vec<f32>) {
        let chunk = TimestampedChunk::new(samples, self.sample_rate);

        // Detect gaps (significant delay between chunks)
        if let Some(last_time) = self.last_chunk_time {
            let gap_duration = last_time.elapsed();
            let expected_duration = Duration::from_secs_f64(
                chunk.samples.len() as f64 / self.sample_rate as f64
            );

            // Gap threshold: 2x expected chunk duration
            if gap_duration > expected_duration.mul_f32(2.0) {
                self.gaps_detected += 1;

                if self.device_kind.is_bluetooth() {
                    debug!("⚠️ Gap detected in '{}': {:.1}ms (expected ~{:.1}ms)",
                           self.device_name,
                           gap_duration.as_secs_f64() * 1000.0,
                           expected_duration.as_secs_f64() * 1000.0);
                } else {
                    warn!("⚠️ Unexpected gap in wired device '{}': {:.1}ms",
                          self.device_name,
                          gap_duration.as_secs_f64() * 1000.0);
                }
            }
        }

        self.total_samples += chunk.samples.len();
        self.chunks.push_back(chunk);
        self.chunks_received += 1;
        self.last_chunk_time = Some(Instant::now());
    }

    /// Check if buffer has data ready (timeout-aware)
    fn has_data(&self) -> bool {
        if let Some(oldest_chunk) = self.chunks.front() {
            // Data is ready if oldest chunk has exceeded timeout
            oldest_chunk.age() >= self.buffer_timeout
        } else {
            false
        }
    }

    /// Pop samples from the buffer (returns None if not ready)
    fn pop_samples(&mut self, sample_count: usize) -> Option<Vec<f32>> {
        if !self.has_data() {
            return None;
        }

        let mut result = Vec::with_capacity(sample_count);

        while result.len() < sample_count {
            if let Some(chunk) = self.chunks.front_mut() {
                let remaining = sample_count - result.len();
                let available = chunk.samples.len();

                if available <= remaining {
                    // Consume entire chunk
                    result.extend_from_slice(&chunk.samples);
                    self.total_samples -= chunk.samples.len();
                    self.chunks.pop_front();
                } else {
                    // Consume partial chunk
                    result.extend_from_slice(&chunk.samples[..remaining]);
                    chunk.samples.drain(..remaining);
                    self.total_samples -= remaining;
                    break;
                }
            } else {
                // No more chunks - insert silence for gap
                let silence_count = sample_count - result.len();
                result.resize(sample_count, 0.0);
                self.silence_inserted_samples += silence_count as u64;

                debug!("🔇 Inserted {:.1}ms silence for '{}' (buffer underrun)",
                       (silence_count as f64 / self.sample_rate as f64) * 1000.0,
                       self.device_name);
                break;
            }
        }

        Some(result)
    }

    /// Get current buffer size in samples
    fn buffer_size(&self) -> usize {
        self.total_samples
    }

    /// Get buffer latency in milliseconds
    fn buffer_latency_ms(&self) -> f64 {
        (self.total_samples as f64 / self.sample_rate as f64) * 1000.0
    }

    /// Get statistics for diagnostics
    fn stats(&self) -> BufferStats {
        BufferStats {
            device_name: self.device_name.clone(),
            device_kind: self.device_kind,
            buffer_size: self.total_samples,
            buffer_latency_ms: self.buffer_latency_ms(),
            chunks_received: self.chunks_received,
            gaps_detected: self.gaps_detected,
            silence_inserted_ms: (self.silence_inserted_samples as f64 / self.sample_rate as f64) * 1000.0,
        }
    }
}

/// Buffer statistics for diagnostics
#[derive(Debug, Clone)]
pub struct BufferStats {
    pub device_name: String,
    pub device_kind: InputDeviceKind,
    pub buffer_size: usize,
    pub buffer_latency_ms: f64,
    pub chunks_received: u64,
    pub gaps_detected: u32,
    pub silence_inserted_ms: f64,
}

/// Professional audio mixer with RMS-based ducking
struct AudioMixer {
    /// Mic ducking factor (0.0 - 1.0)
    mic_ducking: f32,

    /// System audio ducking factor (0.0 - 1.0)
    system_ducking: f32,

    /// Enable RMS-based adaptive ducking
    adaptive_ducking: bool,
}

impl AudioMixer {
    fn new(adaptive_ducking: bool) -> Self {
        Self {
            mic_ducking: 1.0,      // Full volume by default
            system_ducking: 0.60,   // System audio at 40% when mic is active
            adaptive_ducking,
        }
    }

    /// Mix mic and system audio with professional ducking
    ///
    /// Strategy:
    /// - When mic is active (speech detected), duck system audio
    /// - When mic is silent, allow full system audio
    /// - Use RMS to detect speech activity
    fn mix(&mut self, mic: &[f32], system: &[f32]) -> Vec<f32> {
        assert_eq!(mic.len(), system.len(), "Mic and system audio must have same length");

        let mut result = Vec::with_capacity(mic.len());

        if self.adaptive_ducking {
            // Calculate RMS for mic to detect speech
            let mic_rms = calculate_rms(mic);

            // Speech detection threshold (calibrated for meetings)
            const SPEECH_THRESHOLD: f32 = 0.01;

            let is_speech = mic_rms > SPEECH_THRESHOLD;

            // Adjust ducking based on speech detection
            let system_gain = if is_speech {
                self.system_ducking  // Duck system audio when mic has speech
            } else {
                1.0  // Full system audio when mic is silent
            };

            // Mix with ducking
            for (m, s) in mic.iter().zip(system.iter()) {
                let mixed = (m * self.mic_ducking) + (s * system_gain);
                // Prevent clipping
                result.push(mixed.clamp(-1.0, 1.0));
            }
        } else {
            // Simple mixing without ducking
            for (m, s) in mic.iter().zip(system.iter()) {
                let mixed = m + s;
                // Prevent clipping
                result.push(mixed.clamp(-1.0, 1.0));
            }
        }

        result
    }
}

/// FFmpeg-style adaptive audio mixer
///
/// This mixer maintains separate buffers for mic and system audio,
/// with adaptive timeouts based on device characteristics.
pub struct FFmpegAudioMixer {
    mic_buffer: SourceBuffer,
    system_buffer: SourceBuffer,
    mixer: AudioMixer,
    #[allow(dead_code)]
    sample_rate: u32,

    // Mixing window size (50ms by default, matching Cap)
    mixing_window_samples: usize,

    // Statistics
    windows_mixed: u64,
}

impl FFmpegAudioMixer {
    /// Create a new FFmpeg-style adaptive mixer
    ///
    /// # Arguments
    /// * `mic_device_name` - Name of microphone device
    /// * `mic_device_kind` - Detected microphone device type
    /// * `system_device_name` - Name of system audio device
    /// * `system_device_kind` - Detected system audio device type
    /// * `sample_rate` - Sample rate in Hz (should be 48000)
    pub fn new(
        mic_device_name: String,
        mic_device_kind: InputDeviceKind,
        system_device_name: String,
        system_device_kind: InputDeviceKind,
        sample_rate: u32,
    ) -> Self {
        info!("🎛️ Creating FFmpeg Adaptive Audio Mixer");
        info!("   Microphone: '{}' ({:?})", mic_device_name, mic_device_kind);
        info!("   System Audio: '{}' ({:?})", system_device_name, system_device_kind);
        info!("   Sample Rate: {} Hz", sample_rate);

        // 50ms mixing window (same as Cap)
        let mixing_window_samples = ((sample_rate as f64 * 0.050) as usize).max(1);
        info!("   Mixing Window: {:.1}ms ({} samples)",
              (mixing_window_samples as f64 / sample_rate as f64) * 1000.0,
              mixing_window_samples);

        Self {
            mic_buffer: SourceBuffer::new(mic_device_name, mic_device_kind, sample_rate),
            system_buffer: SourceBuffer::new(system_device_name, system_device_kind, sample_rate),
            mixer: AudioMixer::new(true),  // Enable adaptive ducking
            sample_rate,
            mixing_window_samples,
            windows_mixed: 0,
        }
    }

    /// Push microphone audio chunk
    pub fn push_mic(&mut self, samples: Vec<f32>) {
        self.mic_buffer.push(samples);
    }

    /// Push system audio chunk
    pub fn push_system(&mut self, samples: Vec<f32>) {
        self.system_buffer.push(samples);
    }

    /// Check if mixer has data ready to mix
    pub fn has_data_ready(&self) -> bool {
        self.mic_buffer.has_data() && self.system_buffer.has_data()
    }

    /// Pop mixed audio (returns None if not ready)
    ///
    /// Returns a 50ms window of mixed audio when both sources are ready
    pub fn pop_mixed(&mut self) -> Option<Vec<f32>> {
        if !self.has_data_ready() {
            return None;
        }

        // Pop mixing window from both sources
        let mic_samples = self.mic_buffer.pop_samples(self.mixing_window_samples)?;
        let system_samples = self.system_buffer.pop_samples(self.mixing_window_samples)?;

        // Mix the samples
        let mixed = self.mixer.mix(&mic_samples, &system_samples);

        self.windows_mixed += 1;

        // Log statistics periodically
        if self.windows_mixed % 200 == 0 {  // Every ~10 seconds at 50ms windows
            self.log_stats();
        }

        Some(mixed)
    }

    /// Get current buffer statistics
    pub fn get_stats(&self) -> (BufferStats, BufferStats) {
        (self.mic_buffer.stats(), self.system_buffer.stats())
    }

    /// Log mixer statistics
    fn log_stats(&self) {
        let (mic_stats, sys_stats) = self.get_stats();

        info!("🎛️ Mixer Statistics (after {} windows):", self.windows_mixed);
        info!("   Mic: {:.0}ms buffer, {} gaps, {:.1}ms silence inserted",
              mic_stats.buffer_latency_ms,
              mic_stats.gaps_detected,
              mic_stats.silence_inserted_ms);
        info!("   System: {:.0}ms buffer, {} gaps, {:.1}ms silence inserted",
              sys_stats.buffer_latency_ms,
              sys_stats.gaps_detected,
              sys_stats.silence_inserted_ms);
    }

    /// Get microphone buffer size
    pub fn mic_buffer_size(&self) -> usize {
        self.mic_buffer.buffer_size()
    }

    /// Get system audio buffer size
    pub fn system_buffer_size(&self) -> usize {
        self.system_buffer.buffer_size()
    }
}

/// Calculate RMS (Root Mean Square) for audio samples
fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum_squares: f32 = samples.iter().map(|s| s * s).sum();
    (sum_squares / samples.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_buffer_basic() {
        let mut buffer = SourceBuffer::new(
            "Test Mic".to_string(),
            InputDeviceKind::Wired,
            48000,
        );

        // Push some samples
        buffer.push(vec![0.1, 0.2, 0.3, 0.4]);

        assert_eq!(buffer.buffer_size(), 4);
        assert_eq!(buffer.chunks_received, 1);
    }

    #[test]
    fn test_ffmpeg_mixer_creation() {
        let mixer = FFmpegAudioMixer::new(
            "Test Mic".to_string(),
            InputDeviceKind::Wired,
            "Test System".to_string(),
            InputDeviceKind::Wired,
            48000,
        );

        assert_eq!(mixer.sample_rate, 48000);
        assert_eq!(mixer.mixing_window_samples, 2400);  // 50ms at 48kHz
    }

    #[test]
    fn test_rms_calculation() {
        let samples = vec![0.5, -0.5, 0.5, -0.5];
        let rms = calculate_rms(&samples);
        assert!((rms - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_audio_mixer_clipping_prevention() {
        let mut mixer = AudioMixer::new(false);

        // Test clipping prevention with extreme values
        let mic = vec![0.8, 0.8, 0.8, 0.8];
        let system = vec![0.8, 0.8, 0.8, 0.8];

        let mixed = mixer.mix(&mic, &system);

        // All values should be clamped to 1.0
        for sample in mixed {
            assert!(sample <= 1.0 && sample >= -1.0);
        }
    }
}
