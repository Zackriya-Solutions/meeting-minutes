use std::sync::Arc;
use anyhow::Result;
use log::{error, info, warn};
use tokio::sync::mpsc;

use super::devices::AudioDevice;
use super::pipeline::AudioCapture;
use super::recording_state::{RecordingState, DeviceType};
use super::capture::{SystemAudioCapture, SystemAudioStream};

/// System audio stream implementation that integrates with existing pipeline
pub struct SystemAudioStreamManager {
    device: Arc<AudioDevice>,
    stream: Option<SystemAudioStream>,
    _capture_task: Option<tokio::task::JoinHandle<()>>,
}

impl SystemAudioStreamManager {
    /// Create a new system audio stream that integrates with existing recording pipeline
    pub async fn create(
        device: Arc<AudioDevice>,
        state: Arc<RecordingState>,
        recording_sender: Option<mpsc::UnboundedSender<super::recording_state::AudioChunk>>,
    ) -> Result<Self> {
        info!("Creating system audio stream for device: {}", device.name);

        // Create system audio capture
        let system_capture = SystemAudioCapture::new()?;
        let mut system_stream = system_capture.start_system_audio_capture()?;

        // Create audio capture processor to integrate with existing pipeline
        let audio_capture = AudioCapture::new(
            device.clone(),
            state.clone(),
            system_stream.sample_rate(),
            2, // Assume stereo for system audio
            DeviceType::Output,
            recording_sender,
        );

        // Spawn task to process system audio stream
        let capture_task = tokio::spawn(async move {
            use futures_util::StreamExt;

            let mut buffer = Vec::new();
            let mut frame_count = 0;
            let frames_per_chunk = 1024; // Process in chunks of 1024 samples

            while let Some(sample) = system_stream.next().await {
                buffer.push(sample);
                frame_count += 1;

                // Process when we have enough samples
                if frame_count >= frames_per_chunk {
                    audio_capture.process_audio_data(&buffer);
                    buffer.clear();
                    frame_count = 0;
                }
            }

            // Process any remaining samples
            if !buffer.is_empty() {
                audio_capture.process_audio_data(&buffer);
            }

            info!("System audio capture task ended");
        });

        info!("System audio stream started for device: {}", device.name);

        Ok(Self {
            device,
            stream: Some(system_stream),
            _capture_task: Some(capture_task),
        })
    }

    /// Get device info
    pub fn device(&self) -> &AudioDevice {
        &self.device
    }

    /// Stop the system audio stream
    pub fn stop(mut self) -> Result<()> {
        info!("Stopping system audio stream for device: {}", self.device.name);

        if let Some(stream) = self.stream.take() {
            drop(stream); // This should trigger the stream cleanup
        }

        if let Some(task) = self._capture_task.take() {
            task.abort();
        }

        Ok(())
    }
}

/// Enhanced AudioStreamManager that can use either regular CPAL or our new system audio approach
pub struct EnhancedAudioStreamManager {
    microphone_stream: Option<super::stream::AudioStream>,
    system_stream: Option<SystemAudioStreamManager>,
    state: Arc<RecordingState>,
}

impl EnhancedAudioStreamManager {
    pub fn new(state: Arc<RecordingState>) -> Self {
        Self {
            microphone_stream: None,
            system_stream: None,
            state,
        }
    }

    /// Start audio streams with enhanced system audio capture
    pub async fn start_streams(
        &mut self,
        microphone_device: Option<Arc<AudioDevice>>,
        system_device: Option<Arc<AudioDevice>>,
        recording_sender: Option<mpsc::UnboundedSender<super::recording_state::AudioChunk>>,
    ) -> Result<()> {
        info!("Starting enhanced audio streams");

        // Start microphone stream (if available)
        if let Some(mic_device) = microphone_device {
            info!("Starting microphone stream: {}", mic_device.name);
            let mic_stream = super::stream::AudioStream::create(
                mic_device,
                self.state.clone(),
                DeviceType::Input,
                recording_sender.clone(),
            ).await?;
            self.microphone_stream = Some(mic_stream);
        }

        // Start system audio stream with enhanced capture (if available)
        if let Some(sys_device) = system_device {
            info!("Starting enhanced system audio stream: {}", sys_device.name);

            // Check if we should use enhanced system audio capture
            if should_use_enhanced_system_audio(&sys_device) {
                info!("Using enhanced Core Audio system capture for: {}", sys_device.name);
                let sys_stream = SystemAudioStreamManager::create(
                    sys_device,
                    self.state.clone(),
                    recording_sender,
                ).await?;
                self.system_stream = Some(sys_stream);
            } else {
                info!("Falling back to ScreenCaptureKit for: {}", sys_device.name);
                // Fallback to existing ScreenCaptureKit approach
                let sys_stream = super::stream::AudioStream::create(
                    sys_device,
                    self.state.clone(),
                    DeviceType::Output,
                    recording_sender,
                ).await?;
                // Note: We'd need to store this differently or modify the structure
                warn!("Fallback ScreenCaptureKit stream created but not stored in enhanced manager");
            }
        }

        let mic_count = if self.microphone_stream.is_some() { 1 } else { 0 };
        let sys_count = if self.system_stream.is_some() { 1 } else { 0 };

        info!("Enhanced audio streams started: {} microphone, {} system audio",
               mic_count, sys_count);

        Ok(())
    }

    /// Stop all streams
    pub async fn stop_streams(&mut self) -> Result<()> {
        info!("Stopping enhanced audio streams");

        if let Some(mic_stream) = self.microphone_stream.take() {
            mic_stream.stop()?;
        }

        if let Some(sys_stream) = self.system_stream.take() {
            sys_stream.stop()?;
        }

        info!("Enhanced audio streams stopped");
        Ok(())
    }

    /// Get count of active streams
    pub fn active_stream_count(&self) -> usize {
        let mut count = 0;
        if self.microphone_stream.is_some() {
            count += 1;
        }
        if self.system_stream.is_some() {
            count += 1;
        }
        count
    }
}

/// Determine if we should use enhanced system audio capture
/// This can be based on device name, capabilities, or user preferences
fn should_use_enhanced_system_audio(device: &AudioDevice) -> bool {
    // For now, always use enhanced capture on macOS
    #[cfg(target_os = "macos")]
    {
        // You could add logic here to check device capabilities or user preferences
        // For example, only use enhanced capture for certain device types
        true
    }

    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_use_enhanced_system_audio() {
        let device = Arc::new(AudioDevice::new("Test Device".to_string(), super::super::DeviceType::Output));

        #[cfg(target_os = "macos")]
        assert!(should_use_enhanced_system_audio(&device));

        #[cfg(not(target_os = "macos"))]
        assert!(!should_use_enhanced_system_audio(&device));
    }
}