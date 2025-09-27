use std::sync::Arc;
use tokio::sync::mpsc;
use anyhow::Result;
use log::{error, info, warn};

use super::core::{AudioDevice, default_input_device, default_output_device};
use super::recording_state::{RecordingState, AudioChunk};
use super::pipeline::AudioPipelineManager;
use super::stream::AudioStreamManager;
use super::recording_saver::RecordingSaver;

/// Simplified recording manager that coordinates all audio components
pub struct RecordingManager {
    state: Arc<RecordingState>,
    stream_manager: AudioStreamManager,
    pipeline_manager: AudioPipelineManager,
    recording_saver: RecordingSaver,
}

// SAFETY: RecordingManager contains types that we've marked as Send
unsafe impl Send for RecordingManager {}

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
            recording_saver: RecordingSaver::new(),
        }
    }

    // Remove app handle storage for now - will be passed directly when saving

    /// Start recording with specified devices
    pub async fn start_recording(
        &mut self,
        microphone_device: Option<Arc<AudioDevice>>,
        system_device: Option<Arc<AudioDevice>>,
    ) -> Result<mpsc::UnboundedReceiver<AudioChunk>> {
        info!("Starting recording manager");

        // Set up transcription channel
        let (transcription_sender, transcription_receiver) = mpsc::unbounded_channel::<AudioChunk>();

        // Start recording saver to accumulate audio data
        let save_sender = self.recording_saver.start_accumulation();

        // Start recording state first
        self.state.start_recording()?;

        // Create a combined sender that forwards to both transcription and saving
        let combined_sender = {
            let trans_sender = transcription_sender.clone();
            let save_sender_clone = save_sender.clone();

            let (combined_tx, mut combined_rx) = mpsc::unbounded_channel::<AudioChunk>();

            // Spawn task to forward chunks to both transcription and saving
            tokio::spawn(async move {
                while let Some(chunk) = combined_rx.recv().await {
                    // Send to transcription (original functionality)
                    if let Err(e) = trans_sender.send(chunk.clone()) {
                        warn!("Failed to send chunk to transcription: {}", e);
                    }

                    // Send to saving (new functionality)
                    if let Err(e) = save_sender_clone.send(chunk) {
                        warn!("Failed to send chunk to saving: {}", e);
                    }
                }
            });

            combined_tx
        };

        // Start the audio processing pipeline with dynamic chunk sizing
        // Note: chunk duration parameter is now ignored in favor of dynamic sizing (5s first, 10s subsequent)
        self.pipeline_manager.start(
            self.state.clone(),
            combined_sender,
            0, // Ignored - using dynamic sizing internally
            48000, // 48kHz sample rate
        )?;


        // Give the pipeline a moment to fully initialize before starting streams
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Start audio streams last, after pipeline is ready, passing the recording saver sender
        self.stream_manager.start_streams(microphone_device, system_device, Some(save_sender)).await?;

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

    /// Stop recording and save audio
    pub async fn stop_recording<R: tauri::Runtime>(&mut self, app: &tauri::AppHandle<R>) -> Result<()> {
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

        // Save the recording
        match self.recording_saver.stop_and_save(app).await {
            Ok(Some(file_path)) => {
                info!("Recording saved successfully to: {}", file_path);
            }
            Ok(None) => {
                info!("Recording not saved (auto-save disabled or no audio data)");
            }
            Err(e) => {
                error!("Failed to save recording: {}", e);
                // Don't fail the stop operation if saving fails
            }
        }

        info!("Recording manager stopped");
        Ok(())
    }

    /// Get recording stats from the saver
    pub fn get_recording_stats(&self) -> (usize, u32) {
        self.recording_saver.get_stats()
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

    /// Cleanup all resources without saving
    pub async fn cleanup_without_save(&mut self) {
        if self.is_recording() {
            info!("Stopping recording without saving during cleanup");

            // Stop recording state first
            self.state.stop_recording();

            // Stop audio streams
            if let Err(e) = self.stream_manager.stop_streams() {
                error!("Error stopping audio streams during cleanup: {}", e);
            }

            // Stop audio pipeline
            if let Err(e) = self.pipeline_manager.stop().await {
                error!("Error stopping audio pipeline during cleanup: {}", e);
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