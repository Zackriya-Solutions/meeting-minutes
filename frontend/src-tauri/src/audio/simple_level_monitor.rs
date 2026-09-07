use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Runtime};
use anyhow::Result;
use log::{error, info, warn, debug};
use serde::Serialize;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, SampleRate, StreamConfig};
use tokio::sync::Mutex;

#[derive(Debug, Serialize, Clone)]
pub struct AudioLevelData {
    pub device_name: String,
    pub device_type: String, // "input" or "output"
    pub rms_level: f32,     // RMS level (0.0 to 1.0)
    pub peak_level: f32,    // Peak level (0.0 to 1.0)
    pub is_active: bool,    // Whether audio is being detected
}

#[derive(Debug, Serialize, Clone)]
pub struct AudioLevelUpdate {
    pub timestamp: u64,
    pub levels: Vec<AudioLevelData>,
}

// Simple global monitoring state
static IS_MONITORING: AtomicBool = AtomicBool::new(false);

lazy_static::lazy_static! {
    static ref CURRENT_LEVELS: Arc<Mutex<Vec<AudioLevelData>>> = Arc::new(Mutex::new(Vec::new()));
}

/// Start audio level monitoring for specified devices
pub async fn start_monitoring<R: Runtime>(
    app_handle: AppHandle<R>,
    device_names: Vec<String>,
) -> Result<()> {
    info!("Starting simplified audio level monitoring for devices: {:?}", device_names);

    // Stop any existing monitoring
    IS_MONITORING.store(false, Ordering::SeqCst);

    // Wait a bit for any existing tasks to stop
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Clear previous levels
    {
        let mut levels = CURRENT_LEVELS.lock().await;
        levels.clear();
    }

    // Start new monitoring
    IS_MONITORING.store(true, Ordering::SeqCst);

    let host = cpal::default_host();
    let levels_clone = CURRENT_LEVELS.clone();

    // Spawn audio capture streams in a blocking task (cpal streams are not Send)
    let device_names_clone = device_names.clone();
    std::thread::spawn(move || {
        let mut streams: Vec<cpal::Stream> = Vec::new();

        for device_name in &device_names_clone {
            match find_and_create_stream(&host, device_name, levels_clone.clone()) {
                Ok(stream) => {
                    if let Err(e) = stream.play() {
                        warn!("Failed to start stream for {}: {}", device_name, e);
                    } else {
                        info!("Started audio monitoring for device: {}", device_name);
                        streams.push(stream);
                    }
                }
                Err(e) => {
                    warn!("Failed to create stream for {}: {}", device_name, e);
                }
            }
        }

        // Keep streams alive while monitoring
        while IS_MONITORING.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        info!("Audio monitoring streams stopped");
    });

    // Spawn emission task to send levels to frontend
    let app_handle_clone = app_handle.clone();
    let levels_for_emit = CURRENT_LEVELS.clone();

    tokio::spawn(async move {
        while IS_MONITORING.load(Ordering::SeqCst) {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            let levels = {
                let data = levels_for_emit.lock().await;
                data.clone()
            };

            if !levels.is_empty() {
                let update = AudioLevelUpdate {
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                    levels,
                };

                if let Err(e) = app_handle_clone.emit("audio-levels", &update) {
                    error!("Failed to emit audio levels: {}", e);
                    break;
                }
            }
        }

        info!("Audio level monitoring task ended");
    });

    Ok(())
}

/// Find a device by name and create a monitoring stream
fn find_and_create_stream(
    host: &cpal::Host,
    device_name: &str,
    levels: Arc<Mutex<Vec<AudioLevelData>>>,
) -> Result<cpal::Stream> {
    // Try to find the device in input devices
    if let Ok(input_devices) = host.input_devices() {
        for device in input_devices {
            if let Ok(name) = device.name() {
                if name == device_name {
                    return create_input_stream(&device, device_name, levels);
                }
            }
        }
    }

    // Also try default input device if name matches
    if let Some(device) = host.default_input_device() {
        if let Ok(name) = device.name() {
            if name == device_name || device_name == "default" {
                return create_input_stream(&device, &name, levels);
            }
        }
    }

    Err(anyhow::anyhow!("Device not found: {}", device_name))
}

/// Create an input stream for audio level monitoring
fn create_input_stream(
    device: &cpal::Device,
    device_name: &str,
    levels: Arc<Mutex<Vec<AudioLevelData>>>,
) -> Result<cpal::Stream> {
    let config = device.default_input_config()?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels();
    let sample_format = config.sample_format();

    debug!("Creating audio level stream for {}: {}Hz, {} channels, {:?}",
           device_name, sample_rate, channels, sample_format);

    let stream_config = StreamConfig {
        channels,
        sample_rate: SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let device_name_owned = device_name.to_string();
    let levels_clone = levels.clone();

    match sample_format {
        SampleFormat::F32 => {
            let stream = device.build_input_stream(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    process_audio_levels(data, channels, &device_name_owned, &levels_clone);
                },
                |err| error!("Audio stream error: {}", err),
                None,
            )?;
            Ok(stream)
        }
        SampleFormat::I16 => {
            let stream = device.build_input_stream(
                &stream_config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let f32_data: Vec<f32> = data.iter().map(|&s| s.to_sample()).collect();
                    process_audio_levels(&f32_data, channels, &device_name_owned, &levels_clone);
                },
                |err| error!("Audio stream error: {}", err),
                None,
            )?;
            Ok(stream)
        }
        SampleFormat::U16 => {
            let stream = device.build_input_stream(
                &stream_config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    let f32_data: Vec<f32> = data.iter().map(|&s| s.to_sample()).collect();
                    process_audio_levels(&f32_data, channels, &device_name_owned, &levels_clone);
                },
                |err| error!("Audio stream error: {}", err),
                None,
            )?;
            Ok(stream)
        }
        _ => Err(anyhow::anyhow!("Unsupported sample format: {:?}", sample_format)),
    }
}

/// Process audio data and update levels
fn process_audio_levels(
    data: &[f32],
    channels: u16,
    device_name: &str,
    levels: &Arc<Mutex<Vec<AudioLevelData>>>,
) {
    if data.is_empty() {
        return;
    }

    // Convert to mono by averaging channels
    let mono_data: Vec<f32> = if channels > 1 {
        data.chunks(channels as usize)
            .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        data.to_vec()
    };

    // Calculate RMS level
    let rms = if !mono_data.is_empty() {
        (mono_data.iter().map(|&x| x * x).sum::<f32>() / mono_data.len() as f32).sqrt()
    } else {
        0.0
    };

    // Calculate peak level
    let peak = mono_data.iter().map(|&x| x.abs()).fold(0.0_f32, f32::max);

    // Determine if audio is active (threshold for noise floor)
    let is_active = rms > 0.005;

    let level_entry = AudioLevelData {
        device_name: device_name.to_string(),
        device_type: "input".to_string(),
        rms_level: rms.min(1.0),
        peak_level: peak.min(1.0),
        is_active,
    };

    if let Ok(mut levels_guard) = levels.try_lock() {
        // Remove old entry for this device
        levels_guard.retain(|l| l.device_name != device_name);
        levels_guard.push(level_entry);
    }
}

/// Stop audio level monitoring
pub async fn stop_monitoring() -> Result<()> {
    info!("Stopping simplified audio level monitoring");
    IS_MONITORING.store(false, Ordering::SeqCst);

    // Clear levels
    {
        let mut levels = CURRENT_LEVELS.lock().await;
        levels.clear();
    }

    Ok(())
}

/// Check if currently monitoring
pub fn is_monitoring() -> bool {
    IS_MONITORING.load(Ordering::SeqCst)
}
