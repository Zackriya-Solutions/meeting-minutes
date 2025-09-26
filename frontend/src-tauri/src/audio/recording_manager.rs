use std::sync::Arc;
use tokio::sync::mpsc;
use anyhow::Result;
use log::{error, info, warn};

use super::core::{AudioDevice, default_input_device, default_output_device};
use super::recording_state::{RecordingState, AudioChunk};
use super::pipeline::AudioPipelineManager;
use super::stream::AudioStreamManager;

/// Simplified recording manager that coordinates all audio components
pub struct RecordingManager {
    state: Arc<RecordingState>,
    stream_manager: AudioStreamManager,
    pipeline_manager: AudioPipelineManager,
    transcription_receiver: Option<mpsc::UnboundedReceiver<AudioChunk>>,
}

impl RecordingManager {
    /// Create a new recording manager
    pub fn new() -> Self {
        let state = RecordingState::new();
        let stream_manager = AudioStreamManager::new(state.clone());
        let pipeline_manager = AudioPipelineManager::new();

        Self {
            state,
            stream_manager,
            pipeline_manager,
            transcription_receiver: None,
        }
    }

    /// Start recording with specified devices
    pub async fn start_recording(
        &mut self,
        microphone_device: Option<Arc<AudioDevice>>,
        system_device: Option<Arc<AudioDevice>>,
    ) -> Result<mpsc::UnboundedReceiver<AudioChunk>> {
        info!("Starting recording manager");

        // Set up transcription channel
        let (transcription_sender, transcription_receiver) = mpsc::unbounded_channel::<AudioChunk>();

        // Start the audio processing pipeline
        self.pipeline_manager.start(
            self.state.clone(),
            transcription_sender,
            30000, // 30 second chunks
            48000, // 48kHz sample rate
        )?;

        // Start recording state
        self.state.start_recording()?;

        // Start audio streams
        self.stream_manager.start_streams(microphone_device, system_device).await?;

        info!("Recording manager started successfully with {} active streams",
               self.stream_manager.active_stream_count());

        Ok(transcription_receiver)
    }

    /// Start recording with default devices
    pub async fn start_recording_with_defaults(&mut self) -> Result<mpsc::UnboundedReceiver<AudioChunk>> {
        info!("Starting recording with default devices");

        // Get default devices
        let microphone_device = match default_input_device() {
            Ok(device) => {
                info!("Using default microphone: {}", device.name);
                Some(Arc::new(device))
            }
            Err(e) => {
                warn!("No default microphone available: {}", e);
                None
            }
        };

        let system_device = match default_output_device() {
            Ok(device) => {
                info!("Using default system audio: {}", device.name);
                Some(Arc::new(device))
            }
            Err(e) => {
                warn!("No default system audio available: {}", e);
                None
            }
        };

        // Ensure at least microphone is available
        if microphone_device.is_none() {
            return Err(anyhow::anyhow!("No microphone device available"));
        }

        self.start_recording(microphone_device, system_device).await
    }

    /// Stop recording
    pub async fn stop_recording(&mut self) -> Result<()> {
        info!("Stopping recording manager");

        // Stop recording state first
        self.state.stop_recording();

        // Stop audio streams
        if let Err(e) = self.stream_manager.stop_streams() {
            error!("Error stopping audio streams: {}", e);
        }

        // Stop audio pipeline
        if let Err(e) = self.pipeline_manager.stop().await {
            error!("Error stopping audio pipeline: {}", e);
        }

        info!("Recording manager stopped");
        Ok(())
    }

    /// Check if currently recording
    pub fn is_recording(&self) -> bool {
        self.state.is_recording()
    }

    /// Get recording statistics
    pub fn get_stats(&self) -> super::recording_state::RecordingStats {
        self.state.get_stats()
    }

    /// Get recording duration
    pub fn get_recording_duration(&self) -> Option<f64> {
        self.state.get_recording_duration()
    }

    /// Get error information
    pub fn get_error_info(&self) -> (u32, Option<super::recording_state::AudioError>) {
        (self.state.get_error_count(), self.state.get_last_error())
    }

    /// Get active stream count
    pub fn active_stream_count(&self) -> usize {
        self.stream_manager.active_stream_count()
    }

    /// Set error callback for handling errors
    pub fn set_error_callback<F>(&self, callback: F)
    where
        F: Fn(&super::recording_state::AudioError) + Send + Sync + 'static,
    {
        self.state.set_error_callback(callback);
    }

    /// Check if there's a fatal error
    pub fn has_fatal_error(&self) -> bool {
        self.state.has_fatal_error()
    }

    /// Cleanup all resources
    pub async fn cleanup(&mut self) {
        if self.is_recording() {
            if let Err(e) = self.stop_recording().await {
                error!("Error during cleanup: {}", e);
            }
        }
        self.state.cleanup();
    }
}

impl Default for RecordingManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RecordingManager {
    fn drop(&mut self) {
        // Note: Can't call async cleanup in Drop, but streams have their own Drop implementations
        self.state.cleanup();
    }
}