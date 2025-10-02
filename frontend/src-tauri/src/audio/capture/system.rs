use std::pin::Pin;
use std::task::{Context, Poll};
use futures_util::{Stream, StreamExt};
use futures_channel::mpsc;
use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};

/// System audio capture using CPAL (cross-platform)
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
        // Note: System audio capture (loopback) is complex on macOS and requires special configuration
        // For now, this is a placeholder that demonstrates the interface
        // Real system audio capture would require either:
        // 1. Creating an aggregate device with system audio
        // 2. Using SoundFlower or similar virtual audio driver
        // 3. Using private APIs or system extensions

        tracing::warn!("System audio capture is not yet fully implemented. This is a placeholder.");

        let (tx, rx) = mpsc::unbounded::<Vec<f32>>();
        let (drop_tx, _drop_rx) = std::sync::mpsc::channel();

        // Create a dummy stream that produces silence for now
        std::thread::spawn(move || {
            let mut tx = tx;
            loop {
                // Send some silence - this is just a placeholder
                let silence = vec![0.0f32; 1024];
                if tx.start_send(silence).is_err() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        });

        let receiver = rx.map(futures_util::stream::iter).flatten();

        Ok(SystemAudioStream {
            drop_tx,
            sample_rate: 44100, // Default sample rate
            receiver: Box::pin(receiver),
        })
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