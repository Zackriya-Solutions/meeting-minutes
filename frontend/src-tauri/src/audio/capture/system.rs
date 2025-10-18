use std::pin::Pin;
use std::task::{Context, Poll};
use futures_util::{Stream, StreamExt};
use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};
use futures_channel::mpsc;
use log::info;

#[cfg(target_os = "windows")]
use cpal::traits::StreamTrait;

#[cfg(target_os = "windows")]
use log::error;

#[cfg(target_os = "macos")]
use super::core_audio::CoreAudioCapture;

/// System audio capture using Core Audio tap (macOS) or CPAL (other platforms)
pub struct SystemAudioCapture {
    _host: cpal::Host,
}

impl SystemAudioCapture {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        Ok(Self { _host: host })
    }

    pub fn list_system_devices() -> Result<Vec<String>> {
        let host = cpal::default_host();
        let devices = host.output_devices()
            .map_err(|e| anyhow::anyhow!("Failed to enumerate output devices: {}", e))?;

        let mut device_names = Vec::new();
        for device in devices {
            if let Ok(name) = device.name() {
                device_names.push(name);
            }
        }

        Ok(device_names)
    }

    pub fn start_system_audio_capture(&self) -> Result<SystemAudioStream> {
        #[cfg(target_os = "macos")]
        {
            info!("Starting Core Audio system capture (macOS)");
            // Use Core Audio tap for system audio capture
            let core_audio = CoreAudioCapture::new()?;
            let core_audio_stream = core_audio.stream()?;
            let sample_rate = core_audio_stream.sample_rate();

            // Convert CoreAudioStream to SystemAudioStream
            let (tx, rx) = mpsc::unbounded::<Vec<f32>>();
            let (drop_tx, drop_rx) = std::sync::mpsc::channel::<()>();

            // Spawn task to forward Core Audio samples
            tokio::spawn(async move {
                use futures_util::StreamExt;
                let mut stream = core_audio_stream;
                let mut buffer = Vec::new();
                let chunk_size = 1024;

                loop {
                    // Check if we should stop
                    if drop_rx.try_recv().is_ok() {
                        break;
                    }

                    // Poll the Core Audio stream
                    match stream.next().await {
                        Some(sample) => {
                            buffer.push(sample);
                            if buffer.len() >= chunk_size {
                                if tx.unbounded_send(buffer.clone()).is_err() {
                                    break;
                                }
                                buffer.clear();
                            }
                        }
                        None => break,
                    }
                }

                // Send any remaining samples
                if !buffer.is_empty() {
                    let _ = tx.unbounded_send(buffer);
                }
            });

            let receiver = rx.map(futures_util::stream::iter).flatten();

            info!("Core Audio system capture started successfully");

            Ok(SystemAudioStream {
                drop_tx,
                sample_rate,
                receiver: Box::pin(receiver),
            })
        }

        #[cfg(target_os = "windows")]
        {
            info!("Starting WASAPI loopback system capture (Windows)");
            // Use WASAPI loopback for system audio capture on Windows
            let wasapi_host = cpal::host_from_id(cpal::HostId::Wasapi)
                .map_err(|e| anyhow::anyhow!("Failed to create WASAPI host: {}", e))?;

            // Get the default output device (for loopback capture)
            let device = wasapi_host.default_output_device()
                .ok_or_else(|| anyhow::anyhow!("No default output device found for loopback"))?;

            let device_name = device.name()
                .unwrap_or_else(|_| "Unknown Device".to_string());
            info!("Using Windows loopback device: {}", device_name);

            // Get the device configuration
            let config = device.default_output_config()
                .map_err(|e| anyhow::anyhow!("Failed to get output config: {}", e))?;

            let sample_rate = config.sample_rate().0;
            let channels = config.channels();
            info!("WASAPI loopback config - Sample rate: {}, Channels: {}, Format: {:?}",
                  sample_rate, channels, config.sample_format());

            // Create channel for audio samples
            let (tx, rx) = mpsc::unbounded::<Vec<f32>>();
            let (drop_tx, drop_rx) = std::sync::mpsc::channel::<()>();

            // Build input stream in loopback mode (WASAPI captures output as input)
            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => {
                    let tx_clone = tx.clone();
                    device.build_input_stream(
                        &config.into(),
                        move |data: &[f32], _: &cpal::InputCallbackInfo| {
                            // Send audio data through channel
                            if tx_clone.unbounded_send(data.to_vec()).is_err() {
                                error!("Failed to send WASAPI loopback audio data");
                            }
                        },
                        move |err| {
                            error!("WASAPI loopback stream error: {}", err);
                        },
                        None,
                    )
                    .map_err(|e| anyhow::anyhow!("Failed to build F32 loopback stream: {}", e))?
                }
                cpal::SampleFormat::I16 => {
                    let tx_clone = tx.clone();
                    device.build_input_stream(
                        &config.into(),
                        move |data: &[i16], _: &cpal::InputCallbackInfo| {
                            // Convert I16 to F32
                            let f32_data: Vec<f32> = data.iter()
                                .map(|&sample| sample as f32 / i16::MAX as f32)
                                .collect();
                            if tx_clone.unbounded_send(f32_data).is_err() {
                                error!("Failed to send WASAPI loopback audio data");
                            }
                        },
                        move |err| {
                            error!("WASAPI loopback stream error: {}", err);
                        },
                        None,
                    )
                    .map_err(|e| anyhow::anyhow!("Failed to build I16 loopback stream: {}", e))?
                }
                cpal::SampleFormat::I32 => {
                    let tx_clone = tx.clone();
                    device.build_input_stream(
                        &config.into(),
                        move |data: &[i32], _: &cpal::InputCallbackInfo| {
                            // Convert I32 to F32
                            let f32_data: Vec<f32> = data.iter()
                                .map(|&sample| sample as f32 / i32::MAX as f32)
                                .collect();
                            if tx_clone.unbounded_send(f32_data).is_err() {
                                error!("Failed to send WASAPI loopback audio data");
                            }
                        },
                        move |err| {
                            error!("WASAPI loopback stream error: {}", err);
                        },
                        None,
                    )
                    .map_err(|e| anyhow::anyhow!("Failed to build I32 loopback stream: {}", e))?
                }
                cpal::SampleFormat::I8 => {
                    let tx_clone = tx.clone();
                    device.build_input_stream(
                        &config.into(),
                        move |data: &[i8], _: &cpal::InputCallbackInfo| {
                            // Convert I8 to F32
                            let f32_data: Vec<f32> = data.iter()
                                .map(|&sample| sample as f32 / i8::MAX as f32)
                                .collect();
                            if tx_clone.unbounded_send(f32_data).is_err() {
                                error!("Failed to send WASAPI loopback audio data");
                            }
                        },
                        move |err| {
                            error!("WASAPI loopback stream error: {}", err);
                        },
                        None,
                    )
                    .map_err(|e| anyhow::anyhow!("Failed to build I8 loopback stream: {}", e))?
                }
                _ => {
                    return Err(anyhow::anyhow!("Unsupported sample format: {:?}", config.sample_format()));
                }
            };

            // Start the loopback stream
            stream.play()
                .map_err(|e| anyhow::anyhow!("Failed to start WASAPI loopback stream: {}", e))?;

            // SAFETY: We wrap the stream in a Send-safe wrapper to manage its lifecycle.
            // This is safe because:
            // 1. The WASAPI callbacks run in their own OS-managed thread
            // 2. We only drop the stream from a single dedicated thread
            // 3. The stream is never accessed from multiple threads simultaneously
            #[allow(dead_code)]
            struct SendableStream(cpal::Stream);
            unsafe impl Send for SendableStream {}

            let sendable_stream = SendableStream(stream);

            // Spawn a task to keep the stream alive
            // The stream must stay alive for the callbacks to continue firing
            std::thread::spawn(move || {
                let _stream = sendable_stream;

                // Wait for drop signal
                let _ = drop_rx.recv();

                info!("WASAPI loopback stream task shutting down");
                // Stream will be dropped here, stopping the callbacks
            });

            let receiver = rx.map(futures_util::stream::iter).flatten();

            info!("WASAPI loopback system capture started successfully");

            Ok(SystemAudioStream {
                drop_tx,
                sample_rate,
                receiver: Box::pin(receiver),
            })
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            // For other platforms (Linux, etc.), ALSA/PulseAudio loopback would go here
            anyhow::bail!("System audio capture not yet implemented for this platform")
        }
    }

    pub fn check_system_audio_permissions() -> bool {
        // Check if we can enumerate audio devices
        match cpal::default_host().output_devices() {
            Ok(_) => true,
            Err(_) => false,
        }
    }
}

pub struct SystemAudioStream {
    drop_tx: std::sync::mpsc::Sender<()>,
    sample_rate: u32,
    receiver: Pin<Box<dyn Stream<Item = f32> + Send + Sync>>,
}

impl Drop for SystemAudioStream {
    fn drop(&mut self) {
        let _ = self.drop_tx.send(());
    }
}

impl Stream for SystemAudioStream {
    type Item = f32;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.as_mut().poll_next_unpin(cx)
    }
}

impl SystemAudioStream {
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

/// Public interface for system audio capture
pub async fn start_system_audio_capture() -> Result<SystemAudioStream> {
    let capture = SystemAudioCapture::new()?;
    capture.start_system_audio_capture()
}

pub fn list_system_audio_devices() -> Result<Vec<String>> {
    SystemAudioCapture::list_system_devices()
}

pub fn check_system_audio_permissions() -> bool {
    SystemAudioCapture::check_system_audio_permissions()
}