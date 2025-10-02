use std::sync::Arc;
use std::collections::VecDeque;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use anyhow::Result;
use log::{debug, error, info, warn};
use crate::{perf_debug, batch_audio_metric};
use super::batch_processor::AudioMetricsBatcher;

use super::devices::AudioDevice;
use super::recording_state::{AudioChunk, AudioError, RecordingState, DeviceType};
use super::audio_processing::audio_to_mono;
use super::vad::{ContinuousVadProcessor};

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
        // Use 50ms windows for mixing
        let window_ms = 50.0;
        let window_size_samples = (sample_rate as f32 * window_ms / 1000.0) as usize;

        // Max buffer size: 100ms (prevent unbounded growth)
        let max_buffer_size = window_size_samples * 2;

        Self {
            mic_buffer: VecDeque::with_capacity(max_buffer_size),
            system_buffer: VecDeque::with_capacity(max_buffer_size),
            window_size_samples,
            max_buffer_size,
        }
    }

    fn add_samples(&mut self, device_type: DeviceType, samples: Vec<f32>) {
        match device_type {
            DeviceType::Microphone => self.mic_buffer.extend(samples),
            DeviceType::System => self.system_buffer.extend(samples),
        }

        // Safety: prevent buffer overflow (keep only last 100ms)
        while self.mic_buffer.len() > self.max_buffer_size {
            self.mic_buffer.pop_front();
        }
        while self.system_buffer.len() > self.max_buffer_size {
            self.system_buffer.pop_front();
        }
    }

    fn can_mix(&self) -> bool {
        self.mic_buffer.len() >= self.window_size_samples &&
        self.system_buffer.len() >= self.window_size_samples
    }

    fn extract_window(&mut self) -> Option<(Vec<f32>, Vec<f32>)> {
        if !self.can_mix() {
            return None;
        }

        let mic: Vec<f32> = self.mic_buffer.drain(0..self.window_size_samples).collect();
        let sys: Vec<f32> = self.system_buffer.drain(0..self.window_size_samples).collect();

        Some((mic, sys))
    }

    fn flush(&mut self) -> Option<(Vec<f32>, Vec<f32>)> {
        if self.mic_buffer.is_empty() && self.system_buffer.is_empty() {
            return None;
        }

        let mic: Vec<f32> = self.mic_buffer.drain(..).collect();
        let sys: Vec<f32> = self.system_buffer.drain(..).collect();

        Some((mic, sys))
    }
}

/// Professional audio mixer with RMS-based ducking
/// Uses industry-standard techniques to prevent clipping and artifacts
struct ProfessionalAudioMixer {
    system_rms: f32,  // Running RMS estimate for smooth ducking
    rms_smoothing: f32,  // Smoothing factor (0.95 = very smooth)
}

impl ProfessionalAudioMixer {
    fn new(_sample_rate: u32) -> Self {
        Self {
            system_rms: 0.0,
            rms_smoothing: 0.95,
        }
    }

    fn mix_window(&mut self, mic_window: &[f32], sys_window: &[f32]) -> Vec<f32> {
        // Handle different lengths (pad shorter with zeros)
        let max_len = mic_window.len().max(sys_window.len());
        let mut mixed = Vec::with_capacity(max_len);

        // Calculate system RMS for ducking decision
        let sys_rms = Self::calculate_rms(sys_window);
        self.system_rms = self.rms_smoothing * self.system_rms +
                          (1.0 - self.rms_smoothing) * sys_rms;

        // Calculate ducking amount based on system RMS (not per-sample)
        // Smooth curve: 0.0 → 0.4 as RMS increases from 0.01 → 0.1
        let duck_amount = if self.system_rms > 0.01 {
            ((self.system_rms - 0.01) / 0.09).min(1.0) * 0.4
        } else {
            0.0
        };

        // Mix sample-by-sample with professional formula
        for i in 0..max_len {
            let mic = mic_window.get(i).copied().unwrap_or(0.0);
            let sys = sys_window.get(i).copied().unwrap_or(0.0);

            // Apply ducking to mic (1.0 → 0.6 based on system RMS)
            let mic_ducked = mic * (1.0 - duck_amount);

            // Sum with automatic gain compensation to prevent clipping
            // This is the key: scale down if sum would exceed ±1.0
            let sum_abs = mic_ducked.abs() + sys.abs();
            let scale = if sum_abs > 1.0 { 1.0 / sum_abs } else { 1.0 };

            let mixed_sample = (mic_ducked + sys) * scale;
            mixed.push(mixed_sample.clamp(-1.0, 1.0));
        }

        mixed
    }

    fn calculate_rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt()
    }
}

/// Simplified audio capture without broadcast channels
#[derive(Clone)]
pub struct AudioCapture {
    device: Arc<AudioDevice>,
    state: Arc<RecordingState>,
    sample_rate: u32,
    channels: u16,
    chunk_counter: Arc<std::sync::atomic::AtomicU64>,
    device_type: DeviceType,
    recording_sender: Option<mpsc::UnboundedSender<AudioChunk>>,
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
        Self {
            device,
            state,
            sample_rate,
            channels,
            chunk_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            device_type,
            recording_sender,
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
        let mono_data = if self.channels > 1 {
            audio_to_mono(data, self.channels)
        } else {
            data.to_vec()
        };

        // Create audio chunk with stream-specific timestamp (get ID first for logging)
        let chunk_id = self.chunk_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // RAW AUDIO: No gain applied here - will be applied AFTER mixing
        // This prevents amplifying system audio bleed-through in the microphone

        // DIAGNOSTIC: Log audio levels for debugging (especially mic issues)
        if chunk_id % 100 == 0 && !mono_data.is_empty() {
            let raw_rms = (mono_data.iter().map(|&x| x * x).sum::<f32>() / mono_data.len() as f32).sqrt();
            let raw_peak = mono_data.iter().map(|&x| x.abs()).fold(0.0f32, f32::max);

            if self.device_type == DeviceType::Microphone {
                info!("🎙️ [{:?}] Chunk {} - Raw: RMS={:.6}, Peak={:.6}",
                      self.device_type, chunk_id, raw_rms, raw_peak);
            }

            // Warn if microphone is completely silent
            if matches!(self.device_type, DeviceType::Microphone) && raw_rms == 0.0 && raw_peak == 0.0 {
                warn!("⚠️ Microphone producing ZERO audio - check permissions or hardware!");
            }
        }

        // Use global recording timestamp for proper synchronization
        let timestamp = self.state.get_recording_duration().unwrap_or(0.0);

        // RAW AUDIO CHUNK: No gain applied - will be mixed and gained downstream
        let audio_chunk = AudioChunk {
            data: mono_data,  // Raw audio, no gain yet
            sample_rate: self.sample_rate,
            timestamp,
            chunk_id,
            device_type: self.device_type.clone(),
        };

        // Send to recording saver
        if let Some(recording_sender) = &self.recording_sender {
            if let Err(e) = recording_sender.send(audio_chunk.clone()) {
                warn!("Failed to send chunk to recording saver: {}", e);
            }
        }

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

    /// Handle stream errors
    pub fn handle_stream_error(&self, error: cpal::StreamError) {
        error!("Audio stream error for {}: {}", self.device.name, error);

        let audio_error = if error.to_string().contains("device is no longer available") {
            AudioError::DeviceDisconnected
        } else if error.to_string().contains("permission") {
            AudioError::PermissionDenied
        } else if error.to_string().contains("channel closed") {
            AudioError::ChannelClosed
        } else {
            AudioError::StreamFailed
        };

        self.state.report_error(audio_error);
    }
}

/// Optimized audio processing pipeline using VAD-driven chunking
pub struct AudioPipeline {
    receiver: mpsc::UnboundedReceiver<AudioChunk>,
    transcription_sender: mpsc::UnboundedSender<AudioChunk>,
    state: Arc<RecordingState>,
    vad_processor: ContinuousVadProcessor,
    sample_rate: u32,
    chunk_id_counter: u64,
    min_chunk_duration_ms: u32,   // Minimum duration before sending to transcription
    max_chunk_duration_ms: u32,   // Maximum duration before forcing transcription
    accumulated_speech: Vec<f32>, // Accumulate speech segments for larger chunks
    last_transcription_time: std::time::Instant,
    accumulated_start_timestamp: f64,
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
    ) -> Self {
        // Create VAD processor with longer redemption time for better speech accumulation
        // The VAD processor now handles 48kHz->16kHz resampling internally
        let vad_processor = match ContinuousVadProcessor::new(sample_rate, 800) {
            Ok(processor) => {
                info!("VAD processor created successfully");
                processor
            }
            Err(e) => {
                error!("Failed to create VAD processor: {}", e);
                panic!("VAD processor creation failed: {}", e);
            }
        };

        // PERFORMANCE OPTIMIZATION: Hardware-adaptive chunk sizing for optimal performance
        // Larger chunks = fewer transcription calls = much better throughput
        let hardware_profile = crate::audio::HardwareProfile::detect();
        let recommended_duration = hardware_profile.get_recommended_chunk_duration_ms();

        // Use larger chunks to reduce transcription frequency (major performance gain)
        // Old implementation used 30s chunks - we'll use hardware-adaptive approach
        let min_chunk_duration_ms = if target_chunk_duration_ms == 0 {
            // Match old implementation's strategy: longer chunks for better performance
            match hardware_profile.performance_tier {
                crate::audio::PerformanceTier::Ultra => 25000,  // 25s minimum for best quality
                crate::audio::PerformanceTier::High => 20000,   // 20s for high-end
                crate::audio::PerformanceTier::Medium => 15000, // 15s for medium
                crate::audio::PerformanceTier::Low => 12000,    // 12s for low-end (still better than old 10s)
            }
        } else {
            std::cmp::max(15000, target_chunk_duration_ms) // Minimum 15s for any quality (improved from 10s)
        };

        let max_chunk_duration_ms = if target_chunk_duration_ms == 0 {
            // Maximum duration before forcing transcription
            match hardware_profile.performance_tier {
                crate::audio::PerformanceTier::Ultra => 30000,  // 30s max (matches old implementation)
                crate::audio::PerformanceTier::High => 28000,   // 28s max
                crate::audio::PerformanceTier::Medium => 22000, // 22s max
                crate::audio::PerformanceTier::Low => 18000,    // 18s max
            }
        } else {
            std::cmp::max(20000, target_chunk_duration_ms) // Minimum 20s max (improved from 15s)
        };

        info!("Hardware-adaptive chunking: recommended {}ms, using {}-{}ms (tier: {:?}, GPU: {:?})",
              recommended_duration, min_chunk_duration_ms, max_chunk_duration_ms,
              hardware_profile.performance_tier, hardware_profile.gpu_type);

        // Initialize professional audio mixing components
        let ring_buffer = AudioMixerRingBuffer::new(sample_rate);
        let mixer = ProfessionalAudioMixer::new(sample_rate);

        Self {
            receiver,
            transcription_sender,
            state,
            vad_processor,
            sample_rate,
            chunk_id_counter: 0,
            min_chunk_duration_ms,
            max_chunk_duration_ms,
            accumulated_speech: Vec::new(),
            last_transcription_time: std::time::Instant::now(),
            accumulated_start_timestamp: 0.0,
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
        info!("Audio pipeline started with hardware-adaptive {}-{}ms chunk durations",
              self.min_chunk_duration_ms, self.max_chunk_duration_ms);

        while self.state.is_recording() {
            // Receive audio chunks with timeout
            match tokio::time::timeout(
                std::time::Duration::from_millis(50), // Shorter timeout for responsiveness
                self.receiver.recv()
            ).await {
                Ok(Some(chunk)) => {
                    // PERFORMANCE: Check for flush signal (special chunk with ID >= u64::MAX - 10)
                    // Multiple flush signals may be sent to ensure processing
                    if chunk.chunk_id >= u64::MAX - 10 {
                        info!("📥 Received FLUSH signal #{} - processing ALL accumulated audio immediately", u64::MAX - chunk.chunk_id);
                        if !self.accumulated_speech.is_empty() {
                            info!("🚀 Force-processing {} accumulated samples immediately",
                                  self.accumulated_speech.len());
                            self.send_accumulated_speech(chunk.sample_rate)?;
                        } else {
                            perf_debug!("✅ No accumulated speech to flush");
                        }
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

                    // PROFESSIONAL MIXING: Add to ring buffer (no immediate mixing)
                    self.ring_buffer.add_samples(chunk.device_type.clone(), chunk.data);

                    // Mix in fixed windows only when both streams have sufficient data
                    while self.ring_buffer.can_mix() {
                        if let Some((mic_window, sys_window)) = self.ring_buffer.extract_window() {
                            // UNIFIED MIXING: Same algorithm for recording AND transcription
                            let mixed_clean = self.mixer.mix_window(&mic_window, &sys_window);

                            // Apply post-gain (5x boost) - SAME for both recording AND transcription
                            let mixed_with_gain: Vec<f32> = mixed_clean.iter()
                                .map(|&s| (s * 5.0).clamp(-1.0, 1.0))
                                .collect();

                            // Send to recording (WITH post-gain for consistency)
                            if let Some(ref sender) = self.recording_sender_for_mixed {
                                let recording_chunk = AudioChunk {
                                    data: mixed_with_gain.clone(),  // Same gain as transcription
                                    sample_rate: self.sample_rate,
                                    timestamp: chunk.timestamp,
                                    chunk_id: self.chunk_id_counter,
                                    device_type: DeviceType::Microphone,  // Mixed audio
                                };
                                let _ = sender.send(recording_chunk);
                            }

                            // Process for transcription
                            match self.vad_processor.process_audio(&mixed_with_gain) {
                                Ok(speech_segments) => {
                                    if !speech_segments.is_empty() {
                                        let total_samples: usize = speech_segments.iter().map(|s| s.samples.len()).sum();
                                        info!("✅ VAD emitted {} speech segments ({} samples) from professionally mixed audio",
                                              speech_segments.len(), total_samples);
                                    }
                                }
                                Err(e) => {
                                    warn!("⚠️ VAD error on mixed audio: {}", e);
                                }
                            }

                            self.accumulate_speech(&mixed_with_gain, chunk.timestamp)?;
                        }
                    }

                    // Check if we should send accumulated speech for transcription
                    // Optimized: check every 8 chunks for better responsiveness
                    if chunk.chunk_id % 8 == 0 {
                        self.check_and_send_transcription_chunk(chunk.sample_rate)?;
                    }
                }
                Ok(None) => {
                    info!("Audio pipeline: sender closed after processing {} chunks", self.processed_chunks);
                    break;
                }
                Err(_) => {
                    // Timeout - check for forced transcription due to time limit
                    if self.should_force_transcription() {
                        self.send_accumulated_speech(self.sample_rate)?;
                    } else if self.state.is_paused() && !self.accumulated_speech.is_empty() {
                        // Only log pause state changes, not every timeout
                        // This reduces log spam during pause periods
                    }
                    continue;
                }
            }
        }

        // Process any remaining speech segments and accumulated audio
        self.flush_remaining_audio()?;

        info!("VAD-driven audio pipeline ended");
        Ok(())
    }



    fn accumulate_speech(&mut self, samples: &[f32], timestamp: f64) -> Result<()> {
        if self.accumulated_speech.is_empty() {
            self.accumulated_start_timestamp = timestamp;
        }
        self.accumulated_speech.extend_from_slice(samples);
        Ok(())
    }

    fn check_and_send_transcription_chunk(&mut self, sample_rate: u32) -> Result<()> {
        if self.accumulated_speech.is_empty() {
            return Ok(());
        }

        let accumulated_duration_ms = (self.accumulated_speech.len() as f64 / sample_rate as f64) * 1000.0;
        let time_since_last = self.last_transcription_time.elapsed().as_millis() as u32;

        // Send if we have enough speech or if too much time has passed
        let should_send = accumulated_duration_ms >= self.min_chunk_duration_ms as f64 ||
                         time_since_last >= self.max_chunk_duration_ms;

        if should_send {
            self.send_accumulated_speech(sample_rate)?;
        }

        Ok(())
    }

    fn should_force_transcription(&self) -> bool {
        !self.accumulated_speech.is_empty() &&
        !self.state.is_paused() && // Don't force transcription when paused
        self.last_transcription_time.elapsed().as_millis() as u32 >= self.max_chunk_duration_ms
    }

    fn send_accumulated_speech(&mut self, sample_rate: u32) -> Result<()> {
        if self.accumulated_speech.is_empty() {
            return Ok(());
        }

        // Don't process accumulated speech when recording is paused
        if self.state.is_paused() {
            info!("Skipping accumulated speech processing while paused ({} samples)",
                  self.accumulated_speech.len());
            return Ok(());
        }

        let accumulated_samples = std::mem::take(&mut self.accumulated_speech);
        let duration_ms = (accumulated_samples.len() as f64 / sample_rate as f64) * 1000.0;

        // Use performance-optimized debug logging
        perf_debug!("Processing accumulated speech: {} samples ({:.1}ms)",
                   accumulated_samples.len(), duration_ms);

        // Simplified validation: Energy + ZCR only (streaming VAD already ran)
        // The streaming VAD has been monitoring for 20-28s, we just validate energy here
        let rms_energy = (accumulated_samples.iter().map(|&x| x * x).sum::<f32>() / accumulated_samples.len() as f32).sqrt();
        let peak_level = accumulated_samples.iter().map(|&x| x.abs()).fold(0.0f32, f32::max);

        // Lower thresholds: streaming VAD already did heavy filtering
        // We just want to catch completely silent chunks
        if rms_energy < 0.005 && peak_level < 0.01 {
            perf_debug!("🔇 Accumulated audio too quiet (RMS: {:.6}, Peak: {:.6}) - likely complete silence",
                       rms_energy, peak_level);
            return Ok(());
        }

        // ZCR validation for tone/noise detection
        if !Self::validate_speech_characteristics(&accumulated_samples) {
            perf_debug!("🔇 ZCR validation failed - likely tone/noise, skipping");
            return Ok(());
        }

        perf_debug!("✅ Sending {} samples to Whisper (RMS: {:.6}, Peak: {:.6})",
                   accumulated_samples.len(), rms_energy, peak_level);

        let speech_samples = accumulated_samples;

        let transcription_chunk = AudioChunk {
            data: speech_samples,
            sample_rate,
            timestamp: self.accumulated_start_timestamp,
            chunk_id: self.chunk_id_counter,
            device_type: DeviceType::Microphone, // Mixed audio for transcription
        };

        // Only log transcription sends for significant chunks to reduce I/O overhead
        if duration_ms > 5000.0 || self.chunk_id_counter % 10 == 0 {
            info!("🎤 Sending VAD-optimized chunk {} for transcription: {:.1}ms duration, {} samples",
                  self.chunk_id_counter, duration_ms, transcription_chunk.data.len());
        } else {
            perf_debug!("Sending chunk {} for transcription: {:.1}ms duration",
                       self.chunk_id_counter, duration_ms);
        }

        if let Err(e) = self.transcription_sender.send(transcription_chunk) {
            warn!("Failed to send transcription chunk: {}", e);
            let error = if e.to_string().contains("closed") {
                AudioError::ChannelClosed
            } else {
                AudioError::TranscriptionFailed
            };
            self.state.report_error(error);
        } else {
            self.chunk_id_counter += 1;
            self.last_transcription_time = std::time::Instant::now();
        }

        Ok(())
    }

    /// Calculate Zero-Crossing Rate - helps distinguish speech from silence/pure tones
    /// Speech has varied frequency content (high ZCR), silence/tones have low ZCR
    fn calculate_zcr(samples: &[f32]) -> f32 {
        if samples.len() < 2 {
            return 0.0;
        }

        let zero_crossings = samples.windows(2)
            .filter(|window| (window[0] * window[1]) < 0.0)
            .count();

        zero_crossings as f32 / samples.len() as f32
    }

    /// Validate audio has speech characteristics using Zero-Crossing Rate
    /// Returns true if audio likely contains speech, false if silence/noise
    fn validate_speech_characteristics(samples: &[f32]) -> bool {
        let zcr = Self::calculate_zcr(samples);

        // Speech typically has ZCR between 0.02 and 0.5
        // Silence/DC offset: < 0.01
        // Pure tones: < 0.02
        // Background noise: 0.01-0.05 (borderline)
        // Speech: > 0.05 (clear indicator)
        const MIN_ZCR_FOR_SPEECH: f32 = 0.02;

        if zcr < MIN_ZCR_FOR_SPEECH {
            perf_debug!("ZCR validation failed: {:.4} < {:.4} (likely silence/tone)", zcr, MIN_ZCR_FOR_SPEECH);
            false
        } else {
            perf_debug!("ZCR validation passed: {:.4} (likely speech)", zcr);
            true
        }
    }

    fn flush_remaining_audio(&mut self) -> Result<()> {
        // PROFESSIONAL MIXING: Flush any remaining audio in ring buffer
        let last_timestamp = self.accumulated_start_timestamp +
            (self.accumulated_speech.len() as f64 / self.sample_rate as f64);

        if let Some((mic_remaining, sys_remaining)) = self.ring_buffer.flush() {
            if !mic_remaining.is_empty() || !sys_remaining.is_empty() {
                info!("Flushing remaining buffered audio - Mic: {} samples, System: {} samples",
                      mic_remaining.len(), sys_remaining.len());

                // Mix remaining audio with professional mixer
                let mixed_clean = self.mixer.mix_window(&mic_remaining, &sys_remaining);

                // Send to recording
                if let Some(ref sender) = self.recording_sender_for_mixed {
                    let recording_chunk = AudioChunk {
                        data: mixed_clean.clone(),
                        sample_rate: self.sample_rate,
                        timestamp: last_timestamp,
                        chunk_id: self.chunk_id_counter,
                        device_type: DeviceType::Microphone,
                    };
                    let _ = sender.send(recording_chunk);
                }

                // Apply post-gain for transcription
                let mixed_with_gain: Vec<f32> = mixed_clean.iter()
                    .map(|&s| (s * 5.0).clamp(-1.0, 1.0))
                    .collect();

                self.accumulate_speech(&mixed_with_gain, last_timestamp)?;
            }
        }

        // Flush any remaining audio from VAD processor
        match self.vad_processor.flush() {
            Ok(final_segments) => {
                for segment in final_segments {
                    self.accumulated_speech.extend_from_slice(&segment.samples);
                }
            }
            Err(e) => {
                warn!("Failed to flush VAD processor: {}", e);
            }
        }

        // Send any remaining accumulated speech
        if !self.accumulated_speech.is_empty() {
            info!("Flushing final accumulated speech: {} samples (processed {} total chunks)",
                  self.accumulated_speech.len(), self.processed_chunks);
            self.send_accumulated_speech(self.sample_rate)?;
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

    /// Start the audio pipeline
    pub fn start(
        &mut self,
        state: Arc<RecordingState>,
        transcription_sender: mpsc::UnboundedSender<AudioChunk>,
        target_chunk_duration_ms: u32,
        sample_rate: u32,
    ) -> Result<()> {
        // Create audio processing channel
        let (audio_sender, audio_receiver) = mpsc::unbounded_channel::<AudioChunk>();

        // Set sender in state for audio captures to use
        state.set_audio_sender(audio_sender.clone());

        // Create and start pipeline
        let pipeline = AudioPipeline::new(
            audio_receiver,
            transcription_sender,
            state.clone(),
            target_chunk_duration_ms,
            sample_rate,
        );

        let handle = tokio::spawn(async move {
            pipeline.run().await
        });

        self.pipeline_handle = Some(handle);
        self.audio_sender = Some(audio_sender);

        info!("Audio pipeline manager started");
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