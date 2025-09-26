use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use anyhow::Result;
use log::{debug, error, info, warn};

use super::core::AudioDevice;
use super::recording_state::{AudioChunk, AudioError, RecordingState};
use super::audio_processing::audio_to_mono;

/// Simplified audio capture without broadcast channels
#[derive(Clone)]
pub struct AudioCapture {
    device: Arc<AudioDevice>,
    state: Arc<RecordingState>,
    sample_rate: u32,
    channels: u16,
    chunk_counter: Arc<std::sync::atomic::AtomicU64>,
}

impl AudioCapture {
    pub fn new(
        device: Arc<AudioDevice>,
        state: Arc<RecordingState>,
        sample_rate: u32,
        channels: u16,
    ) -> Self {
        Self {
            device,
            state,
            sample_rate,
            channels,
            chunk_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
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

        // Create audio chunk
        let chunk_id = self.chunk_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let timestamp = self.state.get_recording_duration().unwrap_or(0.0);

        let chunk = AudioChunk {
            data: mono_data,
            sample_rate: self.sample_rate,
            timestamp,
            chunk_id,
        };

        // Send directly to processing pipeline
        if let Err(e) = self.state.send_audio_chunk(chunk) {
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

/// Audio processing pipeline
pub struct AudioPipeline {
    receiver: mpsc::UnboundedReceiver<AudioChunk>,
    transcription_sender: mpsc::UnboundedSender<AudioChunk>,
    state: Arc<RecordingState>,
    buffer: Vec<f32>,
    target_chunk_size: usize,
}

impl AudioPipeline {
    pub fn new(
        receiver: mpsc::UnboundedReceiver<AudioChunk>,
        transcription_sender: mpsc::UnboundedSender<AudioChunk>,
        state: Arc<RecordingState>,
        target_chunk_duration_ms: u32,
        sample_rate: u32,
    ) -> Self {
        let target_chunk_size = (sample_rate as f32 * (target_chunk_duration_ms as f32 / 1000.0)) as usize;

        Self {
            receiver,
            transcription_sender,
            state,
            buffer: Vec::with_capacity(target_chunk_size * 2),
            target_chunk_size,
        }
    }

    /// Run the audio processing pipeline
    pub async fn run(mut self) -> Result<()> {
        info!("Audio pipeline started");
        let mut chunk_id = 0u64;

        while self.state.is_recording() {
            // Receive audio chunks with timeout
            match tokio::time::timeout(
                std::time::Duration::from_millis(100),
                self.receiver.recv()
            ).await {
                Ok(Some(chunk)) => {
                    debug!("Pipeline received chunk {} with {} samples", chunk.chunk_id, chunk.data.len());
                    self.buffer.extend_from_slice(&chunk.data);

                    // If buffer is large enough, create transcription chunk
                    if self.buffer.len() >= self.target_chunk_size {
                        let transcription_chunk = AudioChunk {
                            data: self.buffer.drain(..self.target_chunk_size).collect(),
                            sample_rate: chunk.sample_rate,
                            timestamp: chunk.timestamp,
                            chunk_id,
                        };

                        if let Err(e) = self.transcription_sender.send(transcription_chunk) {
                            warn!("Failed to send to transcription: {}", e);
                            // More specific error based on transcription failure
                            let error = if e.to_string().contains("closed") {
                                AudioError::ChannelClosed
                            } else {
                                AudioError::TranscriptionFailed
                            };
                            self.state.report_error(error);
                        } else {
                            info!("Sent chunk {} for transcription ({} samples)", chunk_id, self.target_chunk_size);
                            chunk_id += 1;
                        }
                    }
                }
                Ok(None) => {
                    debug!("Audio pipeline: sender closed");
                    break;
                }
                Err(_) => {
                    // Timeout - continue loop to check recording state
                    continue;
                }
            }
        }

        // Process any remaining audio in buffer
        if !self.buffer.is_empty() {
            info!("Processing final buffer with {} samples", self.buffer.len());
            let final_chunk = AudioChunk {
                data: self.buffer.clone(),
                sample_rate: 48000, // Default sample rate
                timestamp: self.state.get_recording_duration().unwrap_or(0.0),
                chunk_id,
            };

            if let Err(e) = self.transcription_sender.send(final_chunk) {
                warn!("Failed to send final chunk: {}", e);
            }
        }

        info!("Audio pipeline ended");
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