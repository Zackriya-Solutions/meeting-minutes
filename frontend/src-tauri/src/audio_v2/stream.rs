//! Async stream-based audio handling
//! 
//! This module provides the foundation for the modern audio system using
//! async streams instead of callbacks and static buffers.

use anyhow::Result;
use futures_util::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::sync::Arc;
use tokio::sync::mpsc;
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, Stream, SupportedStreamConfig};
use log::{error, info, warn};

use crate::audio::core::{AudioDevice, get_device_and_config};
use crate::audio::recording_state::{RecordingState, DeviceType};

/// Processed audio data from the stream
#[derive(Debug, Clone)]
pub struct ProcessedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub timestamp: f64,
    pub device_type: DeviceType,
}

/// Modern async audio stream using futures
pub struct ModernAudioStream {
    device: Arc<AudioDevice>,
    stream: Stream,
    receiver: mpsc::UnboundedReceiver<ProcessedAudio>,
    sample_rate: u32,
    device_type: DeviceType,
}

unsafe impl Send for ModernAudioStream {}

impl ModernAudioStream {
    /// Create a new modern async audio stream
    pub async fn new(
        device: Arc<AudioDevice>,
        device_type: DeviceType,
    ) -> Result<(Self, mpsc::UnboundedSender<ProcessedAudio>)> {
        info!("Creating modern async audio stream for device: {}", device.name);

        // Get the underlying cpal device and config
        let (cpal_device, config) = get_device_and_config(&device).await?;
        let sample_rate = config.sample_rate().0;

        info!("Modern audio config - Sample rate: {}, Channels: {}, Format: {:?}",
              sample_rate, config.channels(), config.sample_format());

        // Create channel for processed audio
        let (sender, receiver) = mpsc::unbounded_channel::<ProcessedAudio>();

        // Create audio processor
        let processor = AudioProcessor::new(
            device.clone(),
            device_type.clone(),
            sample_rate,
            sender.clone(),
        );

        // Build the stream
        let stream = Self::build_stream(&cpal_device, &config, processor)?;

        // Start the stream
        stream.play()?;
        info!("Modern async audio stream started for device: {}", device.name);

        Ok((
            Self {
                device,
                stream,
                receiver,
                sample_rate,
                device_type,
            },
            sender,
        ))
    }

    /// Build stream based on sample format
    fn build_stream(
        device: &Device,
        config: &SupportedStreamConfig,
        processor: AudioProcessor,
    ) -> Result<Stream> {
        let config_copy = config.clone();

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                let processor_clone = processor.clone();
                device.build_input_stream(
                    &config_copy.into(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        processor.process_audio_data(data);
                    },
                    move |err| {
                        processor_clone.handle_stream_error(err);
                    },
                    None,
                )?
            }
            cpal::SampleFormat::I16 => {
                let processor_clone = processor.clone();
                device.build_input_stream(
                    &config_copy.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let f32_data: Vec<f32> = data.iter()
                            .map(|&sample| sample as f32 / i16::MAX as f32)
                            .collect();
                        processor.process_audio_data(&f32_data);
                    },
                    move |err| {
                        processor_clone.handle_stream_error(err);
                    },
                    None,
                )?
            }
            cpal::SampleFormat::I32 => {
                let processor_clone = processor.clone();
                device.build_input_stream(
                    &config_copy.into(),
                    move |data: &[i32], _: &cpal::InputCallbackInfo| {
                        let f32_data: Vec<f32> = data.iter()
                            .map(|&sample| sample as f32 / i32::MAX as f32)
                            .collect();
                        processor.process_audio_data(&f32_data);
                    },
                    move |err| {
                        processor_clone.handle_stream_error(err);
                    },
                    None,
                )?
            }
            cpal::SampleFormat::I8 => {
                let processor_clone = processor.clone();
                device.build_input_stream(
                    &config_copy.into(),
                    move |data: &[i8], _: &cpal::InputCallbackInfo| {
                        let f32_data: Vec<f32> = data.iter()
                            .map(|&sample| sample as f32 / i8::MAX as f32)
                            .collect();
                        processor.process_audio_data(&f32_data);
                    },
                    move |err| {
                        processor_clone.handle_stream_error(err);
                    },
                    None,
                )?
            }
            _ => {
                return Err(anyhow::anyhow!("Unsupported sample format: {:?}", config.sample_format()));
            }
        };

        Ok(stream)
    }

    /// Get device info
    pub fn device(&self) -> &AudioDevice {
        &self.device
    }

    /// Get sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get device type
    pub fn device_type(&self) -> &DeviceType {
        &self.device_type
    }

    /// Stop the stream
    pub fn stop(self) -> Result<()> {
        info!("Stopping modern async audio stream for device: {}", self.device.name);
        drop(self.stream);
        Ok(())
    }
}

impl Stream for ModernAudioStream {
    type Item = ProcessedAudio;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

/// Audio processor for modern streams
#[derive(Clone)]
struct AudioProcessor {
    device: Arc<AudioDevice>,
    device_type: DeviceType,
    sample_rate: u32,
    sender: mpsc::UnboundedSender<ProcessedAudio>,
    chunk_counter: Arc<std::sync::atomic::AtomicU64>,
}

impl AudioProcessor {
    fn new(
        device: Arc<AudioDevice>,
        device_type: DeviceType,
        sample_rate: u32,
        sender: mpsc::UnboundedSender<ProcessedAudio>,
    ) -> Self {
        Self {
            device,
            device_type,
            sample_rate,
            sender,
            chunk_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    fn process_audio_data(&self, data: &[f32]) {
        let chunk_id = self.chunk_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        // Create timestamp based on chunk ID and sample rate
        let timestamp = chunk_id as f64 * data.len() as f64 / self.sample_rate as f64;

        let processed_audio = ProcessedAudio {
            samples: data.to_vec(),
            sample_rate: self.sample_rate,
            timestamp,
            device_type: self.device_type.clone(),
        };

        if let Err(e) = self.sender.send(processed_audio) {
            warn!("Failed to send processed audio: {}", e);
        }
    }

    fn handle_stream_error(&self, error: cpal::StreamError) {
        error!("Audio stream error for {}: {}", self.device.name, error);
    }
}

/// Modern audio stream manager
pub struct ModernAudioStreamManager {
    microphone_stream: Option<ModernAudioStream>,
    system_stream: Option<ModernAudioStream>,
    mic_sender: Option<mpsc::UnboundedSender<ProcessedAudio>>,
    system_sender: Option<mpsc::UnboundedSender<ProcessedAudio>>,
}

unsafe impl Send for ModernAudioStreamManager {}

impl ModernAudioStreamManager {
    pub fn new() -> Self {
        Self {
            microphone_stream: None,
            system_stream: None,
            mic_sender: None,
            system_sender: None,
        }
    }

    /// Start modern audio streams for the given devices
    pub async fn start_streams(
        &mut self,
        microphone_device: Option<Arc<AudioDevice>>,
        system_device: Option<Arc<AudioDevice>>,
    ) -> Result<()> {
        info!("Starting modern async audio streams");

        // Start microphone stream
        if let Some(mic_device) = microphone_device {
            match ModernAudioStream::new(mic_device.clone(), DeviceType::Microphone).await {
                Ok((stream, sender)) => {
                    self.microphone_stream = Some(stream);
                    self.mic_sender = Some(sender);
                    info!("Modern microphone stream started successfully");
                }
                Err(e) => {
                    error!("Failed to create modern microphone stream: {}", e);
                    return Err(e);
                }
            }
        }

        // Start system audio stream
        if let Some(sys_device) = system_device {
            match ModernAudioStream::new(sys_device.clone(), DeviceType::System).await {
                Ok((stream, sender)) => {
                    self.system_stream = Some(stream);
                    self.system_sender = Some(sender);
                    info!("Modern system audio stream started successfully");
                }
                Err(e) => {
                    warn!("Failed to create modern system audio stream: {}", e);
                    // Don't fail if only system audio fails
                }
            }
        }

        // Ensure at least one stream was created
        if self.microphone_stream.is_none() && self.system_stream.is_none() {
            return Err(anyhow::anyhow!("No modern audio streams could be created"));
        }

        Ok(())
    }

    /// Stop all modern audio streams
    pub fn stop_streams(&mut self) -> Result<()> {
        info!("Stopping all modern async audio streams");

        let mut errors = Vec::new();

        // Stop microphone stream
        if let Some(mic_stream) = self.microphone_stream.take() {
            if let Err(e) = mic_stream.stop() {
                error!("Failed to stop modern microphone stream: {}", e);
                errors.push(e);
            }
        }

        // Stop system stream
        if let Some(sys_stream) = self.system_stream.take() {
            if let Err(e) = sys_stream.stop() {
                error!("Failed to stop modern system stream: {}", e);
                errors.push(e);
            }
        }

        // Clear senders
        self.mic_sender = None;
        self.system_sender = None;

        if !errors.is_empty() {
            Err(anyhow::anyhow!("Failed to stop some modern streams: {:?}", errors))
        } else {
            info!("All modern async audio streams stopped successfully");
            Ok(())
        }
    }

    /// Get unified stream that combines both mic and system audio
    pub fn get_unified_stream(&mut self) -> Option<UnifiedAudioStream> {
        if self.microphone_stream.is_some() || self.system_stream.is_some() {
            Some(UnifiedAudioStream {
                mic_stream: self.microphone_stream.as_mut(),
                system_stream: self.system_stream.as_mut(),
            })
        } else {
            None
        }
    }

    /// Get active stream count
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

impl Drop for ModernAudioStreamManager {
    fn drop(&mut self) {
        if let Err(e) = self.stop_streams() {
            error!("Error stopping modern streams during drop: {}", e);
        }
    }
}

/// Unified audio stream that combines microphone and system audio
pub struct UnifiedAudioStream<'a> {
    mic_stream: Option<&'a mut ModernAudioStream>,
    system_stream: Option<&'a mut ModernAudioStream>,
}

impl<'a> Stream for UnifiedAudioStream<'a> {
    type Item = ProcessedAudio;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        // Poll microphone stream first
        if let Some(ref mut mic_stream) = this.mic_stream {
            match Pin::new(mic_stream).poll_next(cx) {
                Poll::Ready(Some(audio)) => return Poll::Ready(Some(audio)),
                Poll::Ready(None) => {
                    // Mic stream ended, remove it
                    this.mic_stream = None;
                }
                Poll::Pending => {}
            }
        }

        // Poll system stream
        if let Some(ref mut system_stream) = this.system_stream {
            match Pin::new(system_stream).poll_next(cx) {
                Poll::Ready(Some(audio)) => return Poll::Ready(Some(audio)),
                Poll::Ready(None) => {
                    // System stream ended, remove it
                    this.system_stream = None;
                }
                Poll::Pending => {}
            }
        }

        // If both streams are gone, we're done
        if this.mic_stream.is_none() && this.system_stream.is_none() {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}
