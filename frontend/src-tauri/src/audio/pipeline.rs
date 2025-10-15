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
use super::audio_processing::{audio_to_mono, LoudnessNormalizer};
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
    sample_rate: u32,        // Original device sample rate
    channels: u16,
    chunk_counter: Arc<std::sync::atomic::AtomicU64>,
    device_type: DeviceType,
    recording_sender: Option<mpsc::UnboundedSender<AudioChunk>>,
    needs_resampling: bool,  // NEW: Flag if resampling is required
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

        if needs_resampling {
            warn!(
                "⚠️ SAMPLE RATE MISMATCH DETECTED ⚠️"
            );
            warn!(
                "🔄 [{:?}] Audio device '{}' reports {} Hz (pipeline expects {} Hz)",
                device_type, device.name, sample_rate, TARGET_SAMPLE_RATE
            );
            warn!(
                "🔄 Automatic resampling will be applied: {} Hz → {} Hz",
                sample_rate, TARGET_SAMPLE_RATE
            );
        } else {
            info!(
                "✅ [{:?}] Audio device '{}' uses {} Hz (matches pipeline)",
                device_type, device.name, sample_rate
            );
        }

        // Initialize EBU R128 normalizer for MICROPHONE ONLY
        // System audio doesn't need normalization (already at good levels)
        let normalizer = if matches!(device_type, DeviceType::Microphone) {
            // Use TARGET_SAMPLE_RATE for normalizer since we resample before normalization
            match LoudnessNormalizer::new(1, TARGET_SAMPLE_RATE) {
                Ok(norm) => {
                    info!("✅ EBU R128 normalizer initialized for microphone '{}' (target: -23 LUFS)", device.name);
                    Some(norm)
                }
                Err(e) => {
                    warn!("⚠️ Failed to create normalizer for microphone: {}, normalization disabled", e);
                    None
                }
            }
        } else {
            // System audio: no normalization
            info!("ℹ️ System audio '{}' will not be normalized (raw capture)", device.name);
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
        // This fixes Bluetooth devices (like Sony WH-1000XM4) that report 16kHz
        // Without this, audio is sped up 3x and VAD fails
        const TARGET_SAMPLE_RATE: u32 = 48000;
        if self.needs_resampling {
            mono_data = super::audio_processing::resample_audio(
                &mono_data,
                self.sample_rate,
                TARGET_SAMPLE_RATE,
            );

            // Log resampling only occasionally to avoid spam
            let chunk_id = self.chunk_counter.load(std::sync::atomic::Ordering::SeqCst);
            if chunk_id % 100 == 0 {
                debug!(
                    "🔄 [{:?}] Resampled chunk {}: {} → {} Hz ({} → {} samples)",
                    self.device_type,
                    chunk_id,
                    self.sample_rate,
                    TARGET_SAMPLE_RATE,
                    data.len() / self.channels as usize,
                    mono_data.len()
                );
            }
        }

        // This ensures normalized audio flows through all paths:
        // - Recording gets normalized mic + raw system (proper balance)
        // - Transcription gets normalized mic + raw system (proper balance)
        // - No complex dual-path logic needed
        if matches!(self.device_type, DeviceType::Microphone) {
            if let Ok(mut normalizer_lock) = self.normalizer.lock() {
                if let Some(ref mut normalizer) = *normalizer_lock {
                    mono_data = normalizer.normalize_loudness(&mono_data);

                    // Log normalization occasionally for debugging
                    let chunk_id = self.chunk_counter.load(std::sync::atomic::Ordering::SeqCst);
                    if chunk_id % 200 == 0 && !mono_data.is_empty() {
                        let rms = (mono_data.iter().map(|&x| x * x).sum::<f32>() / mono_data.len() as f32).sqrt();
                        let peak = mono_data.iter().map(|&x| x.abs()).fold(0.0f32, f32::max);
                        debug!("🎤 Normalized mic chunk {}: RMS={:.4}, Peak={:.4}", chunk_id, rms, peak);
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
        // Create VAD processor with balanced redemption time for speech accumulation
        // The VAD processor now handles 48kHz->16kHz resampling internally
        // Redemption time: 400ms matches Hyprnote's successful configuration
        // This bridges natural pauses without excessive fragmentation
        let vad_processor = match ContinuousVadProcessor::new(sample_rate, 900) {
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
                            // Professional mixing with RMS-based ducking
                            let mixed_clean = self.mixer.mix_window(&mic_window, &sys_window);

                            // Apply post-gain (reduced from 5x to 2x to prevent distortion)
                            let mixed_with_gain: Vec<f32> = mixed_clean.iter()
                                .map(|&s| (s * 2.0).clamp(-1.0, 1.0))
                                .collect();

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
                                    }
                                }
                                Err(e) => {
                                    warn!("⚠️ VAD error: {}", e);
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
        info!("Flushing remaining audio from pipeline (processed {} chunks)", self.processed_chunks);

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
                        };

                        if let Err(e) = self.transcription_sender.send(transcription_chunk) {
                            warn!("Failed to send final VAD segment: {}", e);
                        } else {
                            self.chunk_id_counter += 1;
                        }
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

    /// Start the audio pipeline
    pub fn start(
        &mut self,
        state: Arc<RecordingState>,
        transcription_sender: mpsc::UnboundedSender<AudioChunk>,
        target_chunk_duration_ms: u32,
        sample_rate: u32,
        recording_sender: Option<mpsc::UnboundedSender<AudioChunk>>,
    ) -> Result<()> {
        // Create audio processing channel
        let (audio_sender, audio_receiver) = mpsc::unbounded_channel::<AudioChunk>();

        // Set sender in state for audio captures to use
        state.set_audio_sender(audio_sender.clone());

        // Create and start pipeline
        let mut pipeline = AudioPipeline::new(
            audio_receiver,
            transcription_sender,
            state.clone(),
            target_chunk_duration_ms,
            sample_rate,
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