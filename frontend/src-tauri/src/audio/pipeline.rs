use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use anyhow::Result;
use log::{debug, error, info, warn};

use super::core::AudioDevice;
use super::recording_state::{AudioChunk, AudioError, RecordingState, DeviceType};
use super::audio_processing::audio_to_mono;
use super::vad::{ContinuousVadProcessor};

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

        // Create audio chunk with stream-specific timestamp
        let chunk_id = self.chunk_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // Use global recording timestamp for proper synchronization
        let timestamp = self.state.get_recording_duration().unwrap_or(0.0);

        let chunk = AudioChunk {
            data: mono_data.clone(),
            sample_rate: self.sample_rate,
            timestamp, // Use global recording time for both streams
            chunk_id,
            device_type: self.device_type.clone(),
        };

        // Send raw audio chunk directly to recording saver (bypasses VAD filtering)
        if let Some(recording_sender) = &self.recording_sender {
            if let Err(e) = recording_sender.send(chunk.clone()) {
                warn!("Failed to send chunk to recording saver: {}", e);
            }
        }

        // Send to processing pipeline for transcription
        if let Err(e) = self.state.send_audio_chunk(chunk) {
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

        // Use chunk-based approach like the old implementation
        // - 30 seconds per chunk for better sentence processing (like old code)
        // - This allows proper VAD processing on complete chunks
        let min_chunk_duration_ms = if target_chunk_duration_ms == 0 {
            30000 // 30 seconds like old implementation
        } else {
            std::cmp::max(30000, target_chunk_duration_ms)
        };
        let max_chunk_duration_ms = if target_chunk_duration_ms == 0 {
            30000 // Same as min for consistent chunking
        } else {
            target_chunk_duration_ms
        };

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
        }
    }

    /// Run the VAD-driven audio processing pipeline
    pub async fn run(mut self) -> Result<()> {
        info!("VAD-driven audio pipeline started with {}-{}ms chunk durations",
              self.min_chunk_duration_ms, self.max_chunk_duration_ms);

        while self.state.is_recording() {
            // Receive audio chunks with timeout
            match tokio::time::timeout(
                std::time::Duration::from_millis(50), // Shorter timeout for responsiveness
                self.receiver.recv()
            ).await {
                Ok(Some(chunk)) => {
                    debug!("Pipeline received chunk {} with {} samples", chunk.chunk_id, chunk.data.len());

                    // Use chunk-based approach like old implementation
                    // Accumulate audio in larger chunks before processing
                    self.accumulate_speech(&chunk.data, chunk.timestamp)?;
                    
                    // Check if we should send accumulated speech for transcription
                    // Only check every 10 chunks to allow proper accumulation (like old code)
                    if chunk.chunk_id % 10 == 0 {
                        self.check_and_send_transcription_chunk(chunk.sample_rate)?;
                    }
                }
                Ok(None) => {
                    debug!("Audio pipeline: sender closed");
                    break;
                }
                Err(_) => {
                    // Timeout - check for forced transcription due to time limit
                    if self.should_force_transcription() {
                        self.send_accumulated_speech(self.sample_rate)?;
                    } else if self.state.is_paused() && !self.accumulated_speech.is_empty() {
                        debug!("Audio pipeline: paused with {} accumulated samples - waiting",
                               self.accumulated_speech.len());
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

        info!("Processing accumulated speech: {} samples ({:.1}ms)",
              accumulated_samples.len(), duration_ms);

        // Use old implementation's VAD approach: apply VAD to the complete chunk
        let speech_samples = match crate::audio::vad::extract_speech_16k(&accumulated_samples) {
            Ok(speech) if !speech.is_empty() => {
                info!("VAD extracted {} speech samples from {} total samples", 
                      speech.len(), accumulated_samples.len());
                speech
            }
            Ok(_) => {
                // VAD detected no speech, apply stricter threshold for audio content
                let avg_level = accumulated_samples.iter().map(|&x| x.abs()).sum::<f32>() / accumulated_samples.len() as f32;
                if avg_level > 0.01 { // Increased from 0.003 to 0.01 for better silence filtering
                    info!("VAD detected no speech but significant audio present ({:.6}), including audio", avg_level);
                    accumulated_samples
                } else {
                    info!("VAD detected genuine silence or low-quality audio ({:.6}), skipping", avg_level);
                    return Ok(());
                }
            }
            Err(e) => {
                warn!("VAD error: {}, using original samples", e);
                accumulated_samples
            }
        };

        let transcription_chunk = AudioChunk {
            data: speech_samples,
            sample_rate,
            timestamp: self.accumulated_start_timestamp,
            chunk_id: self.chunk_id_counter,
            device_type: DeviceType::Microphone, // Mixed audio for transcription
        };

        info!("🎤 Sending VAD-optimized chunk {} for transcription: {:.1}ms duration, {} samples",
              self.chunk_id_counter, duration_ms, transcription_chunk.data.len());

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

    fn flush_remaining_audio(&mut self) -> Result<()> {
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
            info!("Flushing final accumulated speech: {} samples", self.accumulated_speech.len());
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
}

impl Default for AudioPipelineManager {
    fn default() -> Self {
        Self::new()
    }
}