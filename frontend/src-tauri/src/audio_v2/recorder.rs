//! Stream-based recording
//! 
//! This module provides a modern recording system that uses async streams
//! instead of static buffers and callbacks.

use anyhow::Result;
use tokio::sync::mpsc;
use std::sync::Arc;
use futures_util::StreamExt;
use log::{info, warn, error};

use super::stream::{ModernAudioStreamManager, ProcessedAudio};
use super::mixer::{AudioMixer, MixingMode};
use super::normalizer::AudioNormalizer;
use super::sync::AudioSynchronizer;
use crate::audio::core::AudioDevice;
use crate::audio::recording_state::DeviceType;

/// Modern recorder using async streams
pub struct ModernRecorder {
    stream_manager: ModernAudioStreamManager,
    mixer: AudioMixer,
    normalizer: AudioNormalizer,
    synchronizer: AudioSynchronizer,
    mic_buffer: Vec<ProcessedAudio>,
    system_buffer: Vec<ProcessedAudio>,
    is_recording: bool,
    sample_rate: u32,
}

impl ModernRecorder {
    /// Create a new modern recorder
    pub fn new(sample_rate: u32) -> Self {
        Self {
            stream_manager: ModernAudioStreamManager::new(),
            mixer: AudioMixer::new(MixingMode::Professional),
            normalizer: AudioNormalizer::new(-23.0), // EBU R128 standard
            synchronizer: AudioSynchronizer::new(1), // 1ms sync tolerance
            mic_buffer: Vec::new(),
            system_buffer: Vec::new(),
            is_recording: false,
            sample_rate,
        }
    }

    /// Start recording with modern async streams
    pub async fn start(
        &mut self,
        microphone_device: Option<Arc<AudioDevice>>,
        system_device: Option<Arc<AudioDevice>>,
    ) -> Result<mpsc::UnboundedSender<ProcessedAudio>> {
        info!("Starting modern recorder with async streams");

        // Start the audio streams
        self.stream_manager.start_streams(microphone_device, system_device).await?;

        // Create channel for processed audio
        let (sender, mut receiver) = mpsc::unbounded_channel::<ProcessedAudio>();

        // Start the recording task
        let mut mixer = self.mixer.clone();
        let mut normalizer = self.normalizer.clone();
        let mut synchronizer = self.synchronizer.clone();
        let mut mic_buffer = Vec::new();
        let mut system_buffer = Vec::new();

        tokio::spawn(async move {
            info!("Modern recording task started");

            while let Some(audio) = receiver.recv().await {
                match audio.device_type {
                    DeviceType::Microphone => {
                        mic_buffer.push(audio);
                    }
                    DeviceType::System => {
                        system_buffer.push(audio);
                    }
                }

                // Process when we have enough data
                if mic_buffer.len() >= 10 || system_buffer.len() >= 10 {
                    Self::process_buffers(
                        &mut mixer,
                        &mut normalizer,
                        &mut synchronizer,
                        &mut mic_buffer,
                        &mut system_buffer,
                    ).await;
                }
            }

            // Process any remaining data
            Self::process_buffers(
                &mut mixer,
                &mut normalizer,
                &mut synchronizer,
                &mut mic_buffer,
                &mut system_buffer,
            ).await;

            info!("Modern recording task completed");
        });

        self.is_recording = true;
        info!("Modern recorder started successfully");

        Ok(sender)
    }

    /// Process audio buffers with modern mixing and normalization
    async fn process_buffers(
        mixer: &mut AudioMixer,
        normalizer: &mut AudioNormalizer,
        synchronizer: &mut AudioSynchronizer,
        mic_buffer: &mut Vec<ProcessedAudio>,
        system_buffer: &mut Vec<ProcessedAudio>,
    ) {
        if mic_buffer.is_empty() && system_buffer.is_empty() {
            return;
        }

        // Extract audio samples
        let mic_samples: Vec<f32> = mic_buffer.iter()
            .flat_map(|audio| &audio.samples)
            .cloned()
            .collect();

        let system_samples: Vec<f32> = system_buffer.iter()
            .flat_map(|audio| &audio.samples)
            .cloned()
            .collect();

        // Mix the audio
        let mixed = mixer.mix(&mic_samples, &system_samples);

        // Normalize the mixed audio
        let normalized = normalizer.normalize(&mixed);

        // TODO: Send to transcription system
        // For now, just log the processing
        info!("Processed {} mic samples and {} system samples into {} mixed samples",
              mic_samples.len(), system_samples.len(), normalized.len());

        // Clear buffers
        mic_buffer.clear();
        system_buffer.clear();
    }

    /// Stop recording and return file path
    pub async fn stop(&mut self) -> Result<Option<String>> {
        info!("Stopping modern recorder");

        if !self.is_recording {
            return Ok(None);
        }

        // Stop the stream manager
        self.stream_manager.stop_streams()?;

        self.is_recording = false;
        info!("Modern recorder stopped successfully");

        // TODO: Implement file saving
        // For now, return None
        Ok(None)
    }

    /// Get recording status
    pub fn is_recording(&self) -> bool {
        self.is_recording
    }

    /// Get audio level statistics
    pub fn get_level_stats(&self) -> super::mixer::AudioLevelStats {
        self.mixer.get_level_stats()
    }

    /// Update mixing mode
    pub fn set_mixing_mode(&mut self, mode: MixingMode) {
        self.mixer.set_mixing_mode(mode);
    }

    /// Get active stream count
    pub fn active_stream_count(&self) -> usize {
        self.stream_manager.active_stream_count()
    }
}

impl Default for ModernRecorder {
    fn default() -> Self {
        Self::new(48000) // Default to 48kHz
    }
}
