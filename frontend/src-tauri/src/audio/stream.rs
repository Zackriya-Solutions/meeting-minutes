use std::sync::Arc;
use anyhow::Result;
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, Stream, SupportedStreamConfig};
use log::{error, info, warn};
use tokio::sync::mpsc;

use super::devices::{AudioDevice, get_device_and_config};
use super::pipeline::AudioCapture;
use super::recording_state::{RecordingState, DeviceType};
use super::capture::{AudioCaptureBackend, get_current_backend};

#[cfg(target_os = "macos")]
use super::capture::CoreAudioCapture;

#[cfg(target_os = "linux")]
use super::capture::{find_monitor_source_by_description, find_source_by_description, PulseCapture};
#[cfg(target_os = "linux")]
use std::sync::atomic::AtomicBool;

/// Stream backend implementation
pub enum StreamBackend {
    /// CPAL-based stream (ScreenCaptureKit or default)
    Cpal(Stream),
    /// Core Audio direct implementation (macOS only)
    #[cfg(target_os = "macos")]
    CoreAudio {
        task: Option<tokio::task::JoinHandle<()>>,
    },
    /// Native PipeWire/PulseAudio implementation (Linux only)
    #[cfg(target_os = "linux")]
    Pulse {
        should_stop: Arc<AtomicBool>,
        task: Option<tokio::task::JoinHandle<()>>,
    },
}

// SAFETY: While Stream doesn't implement Send, we ensure it's only accessed
// from the same thread context by using spawn_blocking for operations that cross thread boundaries
unsafe impl Send for StreamBackend {}

/// Simplified audio stream wrapper with multi-backend support
pub struct AudioStream {
    device: Arc<AudioDevice>,
    backend: StreamBackend,
}

// SAFETY: AudioStream contains StreamBackend which we've marked as Send
unsafe impl Send for AudioStream {}

impl AudioStream {
    /// Create a new audio stream for the given device
    pub async fn create(
        device: Arc<AudioDevice>,
        state: Arc<RecordingState>,
        device_type: DeviceType,
        recording_sender: Option<mpsc::UnboundedSender<super::recording_state::AudioChunk>>,
    ) -> Result<Self> {
        // Get current backend from global config
        let backend_type = get_current_backend();
        Self::create_with_backend(device, state, device_type, recording_sender, backend_type).await
    }

    /// Create a new audio stream with explicit backend selection
    pub async fn create_with_backend(
        device: Arc<AudioDevice>,
        state: Arc<RecordingState>,
        device_type: DeviceType,
        recording_sender: Option<mpsc::UnboundedSender<super::recording_state::AudioChunk>>,
        backend_type: AudioCaptureBackend,
    ) -> Result<Self> {
        info!("🎵 Stream: Creating audio stream for device: {} with backend: {:?}, device_type: {:?}",
              device.name, backend_type, device_type);

        // For system audio devices, use the selected backend.
        // For microphone devices, prefer CPAL, except on Linux where we first
        // try the native PulseAudio/PipeWire source path (see below).
        #[cfg(target_os = "macos")]
        let use_core_audio = device_type == DeviceType::System
            && backend_type == AudioCaptureBackend::CoreAudio;

        #[cfg(not(target_os = "macos"))]
        let use_core_audio = false;

        #[cfg(target_os = "macos")]
        info!("🎵 Stream: use_core_audio = {}, device_type == System: {}, backend == CoreAudio: {}",
              use_core_audio,
              device_type == DeviceType::System,
              backend_type == AudioCaptureBackend::CoreAudio);

        #[cfg(not(target_os = "macos"))]
        info!("🎵 Stream: use_core_audio = {}, device_type == System: {}",
              use_core_audio,
              device_type == DeviceType::System);

        #[cfg(target_os = "macos")]
        if use_core_audio {
            info!("🎵 Stream: Using Core Audio backend (cidre) for system audio");
            return Self::create_core_audio_stream(device, state, device_type, recording_sender).await;
        }

        // Linux system audio always goes through the native PipeWire/PulseAudio
        // path instead of cpal's ALSA host — no backend toggle needed, unlike
        // macOS's ScreenCaptureKit/CoreAudio choice, since there's only one way
        // to capture system audio natively on Linux.
        #[cfg(target_os = "linux")]
        if device_type == DeviceType::System {
            info!("🎵 Stream: Using native PulseAudio/PipeWire backend for system audio");
            return Self::create_pulse_stream(device, state, device_type, recording_sender).await;
        }

        // Linux microphones come from the PulseAudio/PipeWire source list (real
        // human-readable descriptions, see devices/platform/linux.rs). Fall back
        // to CPAL when the name isn't a known Pulse source: stale saved
        // preferences and degraded-mode (no Pulse server) entries must keep
        // working.
        #[cfg(target_os = "linux")]
        if device_type == DeviceType::Microphone {
            match Self::create_pulse_mic_stream(
                device.clone(),
                state.clone(),
                device_type.clone(),
                recording_sender.clone(),
            )
            .await
            {
                Ok(stream) => return Ok(stream),
                Err(e) => warn!(
                    "🎤 Stream: native PulseAudio mic path unavailable ({}), falling back to CPAL",
                    e
                ),
            }
        }

        // Default path: use CPAL
        #[cfg(target_os = "macos")]
        let backend_name = if backend_type == AudioCaptureBackend::ScreenCaptureKit {
            "ScreenCaptureKit"
        } else {
            "CPAL (default)"
        };

        #[cfg(not(target_os = "macos"))]
        let backend_name = "CPAL";

        info!("🎵 Stream: Using CPAL backend ({}) for device: {}", backend_name, device.name);
        Self::create_cpal_stream(device, state, device_type, recording_sender).await
    }

    /// Create a CPAL-based stream (ScreenCaptureKit on macOS)
    async fn create_cpal_stream(
        device: Arc<AudioDevice>,
        state: Arc<RecordingState>,
        device_type: DeviceType,
        recording_sender: Option<mpsc::UnboundedSender<super::recording_state::AudioChunk>>,
    ) -> Result<Self> {
        info!("Creating CPAL stream for device: {}", device.name);

        // Get the underlying cpal device and config
        let (cpal_device, config) = get_device_and_config(&device).await?;

        info!("Audio config - Sample rate: {}, Channels: {}, Format: {:?}",
              config.sample_rate().0, config.channels(), config.sample_format());

        // Create audio capture processor
        let capture = AudioCapture::new(
            device.clone(),
            state.clone(),
            config.sample_rate().0,
            config.channels(),
            device_type,
            recording_sender,
        );

        // Build the appropriate stream based on sample format
        let stream = Self::build_stream(&cpal_device, &config, capture.clone())?;

        // Start the stream
        stream.play()?;
        info!("CPAL stream started for device: {}", device.name);

        Ok(Self {
            device,
            backend: StreamBackend::Cpal(stream),
        })
    }

    /// Create a Core Audio stream (macOS only)
    #[cfg(target_os = "macos")]
    async fn create_core_audio_stream(
        device: Arc<AudioDevice>,
        state: Arc<RecordingState>,
        device_type: DeviceType,
        recording_sender: Option<mpsc::UnboundedSender<super::recording_state::AudioChunk>>,
    ) -> Result<Self> {
        info!("🔊 Stream: Creating Core Audio stream for device: {}", device.name);

        // Create Core Audio capture
        info!("🔊 Stream: Calling CoreAudioCapture::new()...");
        let capture_impl = CoreAudioCapture::new()
            .map_err(|e| {
                error!("❌ Stream: CoreAudioCapture::new() failed: {}", e);
                anyhow::anyhow!("Failed to create Core Audio capture: {}", e)
            })?;

        info!("✅ Stream: CoreAudioCapture created, calling stream()...");
        let core_stream = capture_impl.stream()
            .map_err(|e| {
                error!("❌ Stream: capture_impl.stream() failed: {}", e);
                anyhow::anyhow!("Failed to create Core Audio stream: {}", e)
            })?;

        let sample_rate = core_stream.sample_rate();
        info!("✅ Stream: Core Audio stream created with sample rate: {} Hz", sample_rate);

        // Create audio capture processor for pipeline integration
        // CRITICAL: Core Audio tap is MONO (with_mono_global_tap_excluding_processes)
        let capture = AudioCapture::new(
            device.clone(),
            state.clone(),
            sample_rate,
            1, // Core Audio tap is MONO (not stereo!)
            device_type,
            recording_sender,
        );

        // Spawn task to process Core Audio stream samples
        // The stream needs to be polled continuously to produce samples
        let device_name = device.name.clone();
        info!("🔊 Stream: Spawning tokio task to poll Core Audio stream...");
        let task = tokio::spawn({
            let capture = capture.clone();
            let mut stream = core_stream;

            async move {
                use futures_util::StreamExt;

                let mut buffer = Vec::new();
                let mut frame_count = 0;
                let frames_per_chunk = 1024; // Process in chunks of 1024 samples

                info!("✅ Stream: Core Audio processing task started for {}", device_name);

                let mut _sample_count = 0u64;
                while let Some(sample) = stream.next().await {
                    _sample_count += 1;
                    // if _sample_count % 48000 == 0 {
                    //     info!("📊 Stream: Received {} samples from Core Audio stream", _sample_count);
                    // }

                    buffer.push(sample);
                    frame_count += 1;

                    // Process when we have enough samples
                    if frame_count >= frames_per_chunk {
                        capture.process_audio_data(&buffer);
                        buffer.clear();
                        frame_count = 0;
                    }
                }

                // Process any remaining samples
                if !buffer.is_empty() {
                    capture.process_audio_data(&buffer);
                }

                info!("⚠️ Stream: Core Audio processing task ended for {}", device_name);
            }
        });

        info!("✅ Stream: Core Audio stream fully initialized for device: {}", device.name);

        Ok(Self {
            device: device.clone(),
            backend: StreamBackend::CoreAudio {
                task: Some(task),
            },
        })
    }

    /// Create a native PulseAudio/PipeWire stream (Linux only)
    #[cfg(target_os = "linux")]
    async fn create_pulse_stream(
        device: Arc<AudioDevice>,
        state: Arc<RecordingState>,
        device_type: DeviceType,
        recording_sender: Option<mpsc::UnboundedSender<super::recording_state::AudioChunk>>,
    ) -> Result<Self> {
        info!("🔊 Stream: Creating PulseAudio stream for device: {}", device.name);

        // The picker shows "<sink description> (System Audio)" (see
        // devices/platform/linux.rs); strip that suffix to recover the sink
        // description and resolve it to its real monitor source name.
        let description = device
            .name
            .strip_suffix(" (System Audio)")
            .unwrap_or(&device.name);

        let monitor_source_name = find_monitor_source_by_description(description)
            .map_err(|e| anyhow::anyhow!("Failed to resolve PulseAudio sink '{}': {}", description, e))?;

        let capture_impl = PulseCapture::new_system(&monitor_source_name)
            .map_err(|e| anyhow::anyhow!("Failed to open PulseAudio record stream: {}", e))?;

        let sample_rate = capture_impl.sample_rate();
        let channels = capture_impl.channels();
        let should_stop = capture_impl.stop_handle();

        // Create audio capture processor for pipeline integration
        let capture = AudioCapture::new(
            device.clone(),
            state.clone(),
            sample_rate,
            channels,
            device_type,
            recording_sender,
        );

        let device_name = device.name.clone();
        let task = tokio::task::spawn_blocking(move || {
            info!("✅ Stream: PulseAudio capture thread started for {}", device_name);
            capture_impl.run(|samples| capture.process_audio_data(samples));
            info!("⚠️ Stream: PulseAudio capture thread ended for {}", device_name);
        });

        info!("✅ Stream: PulseAudio stream fully initialized for device: {}", device.name);

        Ok(Self {
            device: device.clone(),
            backend: StreamBackend::Pulse {
                should_stop,
                task: Some(task),
            },
        })
    }

    /// Create a native PulseAudio/PipeWire microphone stream (Linux only)
    #[cfg(target_os = "linux")]
    async fn create_pulse_mic_stream(
        device: Arc<AudioDevice>,
        state: Arc<RecordingState>,
        device_type: DeviceType,
        recording_sender: Option<mpsc::UnboundedSender<super::recording_state::AudioChunk>>,
    ) -> Result<Self> {
        info!(
            "🔊 Stream: Creating PulseAudio microphone stream for device: {}",
            device.name
        );

        // Microphone names are stored as the PulseAudio source description.
        // from_name() has already stripped the "(input)" suffix. If the
        // description isn't known (stale saved preference or degraded-mode
        // entry), this errors out and the caller falls back to CPAL.
        let source_name = find_source_by_description(&device.name)
            .map_err(|e| anyhow::anyhow!("Failed to resolve PulseAudio source '{}': {}", device.name, e))?;

        let capture_impl = PulseCapture::new_microphone(&source_name)
            .map_err(|e| anyhow::anyhow!("Failed to open PulseAudio record stream: {}", e))?;

        let sample_rate = capture_impl.sample_rate();
        let channels = capture_impl.channels();
        let should_stop = capture_impl.stop_handle();

        // Create audio capture processor for pipeline integration
        let capture = AudioCapture::new(
            device.clone(),
            state.clone(),
            sample_rate,
            channels,
            device_type,
            recording_sender,
        );

        let device_name = device.name.clone();
        let task = tokio::task::spawn_blocking(move || {
            info!(
                "✅ Stream: PulseAudio microphone capture thread started for {}",
                device_name
            );
            capture_impl.run(|samples| capture.process_audio_data(samples));
            info!(
                "⚠️ Stream: PulseAudio microphone capture thread ended for {}",
                device_name
            );
        });

        info!(
            "✅ Stream: PulseAudio microphone stream fully initialized for device: {}",
            device.name
        );

        Ok(Self {
            device: device.clone(),
            backend: StreamBackend::Pulse {
                should_stop,
                task: Some(task),
            },
        })
    }

    /// Build stream based on sample format
    fn build_stream(
        device: &Device,
        config: &SupportedStreamConfig,
        capture: AudioCapture,
    ) -> Result<Stream> {
        let config_copy = config.clone();

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                let capture_clone = capture.clone();
                device.build_input_stream(
                    &config_copy.into(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        capture.process_audio_data(data);
                    },
                    move |err| {
                        capture_clone.handle_stream_error(err);
                    },
                    None,
                )?
            }
            cpal::SampleFormat::I16 => {
                let capture_clone = capture.clone();
                device.build_input_stream(
                    &config_copy.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let f32_data: Vec<f32> = data.iter()
                            .map(|&sample| sample as f32 / i16::MAX as f32)
                            .collect();
                        capture.process_audio_data(&f32_data);
                    },
                    move |err| {
                        capture_clone.handle_stream_error(err);
                    },
                    None,
                )?
            }
            cpal::SampleFormat::I32 => {
                let capture_clone = capture.clone();
                device.build_input_stream(
                    &config_copy.into(),
                    move |data: &[i32], _: &cpal::InputCallbackInfo| {
                        let f32_data: Vec<f32> = data.iter()
                            .map(|&sample| sample as f32 / i32::MAX as f32)
                            .collect();
                        capture.process_audio_data(&f32_data);
                    },
                    move |err| {
                        capture_clone.handle_stream_error(err);
                    },
                    None,
                )?
            }
            cpal::SampleFormat::I8 => {
                let capture_clone = capture.clone();
                device.build_input_stream(
                    &config_copy.into(),
                    move |data: &[i8], _: &cpal::InputCallbackInfo| {
                        let f32_data: Vec<f32> = data.iter()
                            .map(|&sample| sample as f32 / i8::MAX as f32)
                            .collect();
                        capture.process_audio_data(&f32_data);
                    },
                    move |err| {
                        capture_clone.handle_stream_error(err);
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

    /// Signal the capture thread to stop, without waiting for it to exit.
    ///
    /// Used only by `AudioStreamManager::Drop`, which cannot be async. Every
    /// normal shutdown path must use `stop().await` instead.
    pub fn signal_stop_only(&self) {
        match &self.backend {
            #[cfg(target_os = "linux")]
            StreamBackend::Pulse { should_stop, .. } => {
                should_stop.store(true, std::sync::atomic::Ordering::Release);
            }
            _ => {
                // CPAL/CoreAudio backends clean themselves up through their own
                // Drop implementations; no explicit signal is needed here.
            }
        }
    }

    /// Stop the stream and wait for its capture thread/task to finish.
    pub async fn stop(self) -> Result<()> {
        info!("Stopping audio stream for device: {}", self.device.name);

        match self.backend {
            StreamBackend::Cpal(stream) => {
                // CRITICAL: Pause the stream first to stop callbacks immediately
                // This ensures closures stop executing before we drop the stream,
                // allowing Arc references captured in callbacks to be released
                if let Err(e) = stream.pause() {
                    warn!("Failed to pause stream before drop: {}", e);
                }
                info!("Stream paused, now dropping to release callbacks");
                drop(stream);
            }
            #[cfg(target_os = "macos")]
            StreamBackend::CoreAudio { task } => {
                // Abort the processing task and wait briefly for cleanup
                if let Some(task_handle) = task {
                    info!("Aborting Core Audio task...");
                    task_handle.abort();
                    // Give the runtime a moment to clean up the aborted task
                    // This helps ensure Arc references in the closure are dropped
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    info!("Core Audio task aborted");
                }
            }
            #[cfg(target_os = "linux")]
            StreamBackend::Pulse { should_stop, task } => {
                // Signal the blocking capture thread to stop after its current
                // read() call returns, then await its JoinHandle with a bounded
                // timeout. abort() on a spawn_blocking task that has already
                // started is a no-op, so waiting is the only reliable way to
                // know the thread has actually exited.
                info!("Signalling PulseAudio capture thread to stop...");
                should_stop.store(true, std::sync::atomic::Ordering::Release);
                if let Some(task_handle) = task {
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(2),
                        task_handle,
                    )
                    .await
                    {
                        Ok(Ok(())) => info!("PulseAudio capture thread joined cleanly"),
                        Ok(Err(e)) => warn!("PulseAudio capture thread panicked: {}", e),
                        Err(_) => warn!(
                            "PulseAudio capture thread did not stop within 2s, abandoning join (thread may still be running)"
                        ),
                    }
                }
            }
        }

        // Explicitly drop self.device Arc reference
        drop(self.device);
        info!("Audio stream stopped and device reference dropped");
        Ok(())
    }
}

/// Audio stream manager for handling multiple streams
pub struct AudioStreamManager {
    microphone_stream: Option<AudioStream>,
    system_stream: Option<AudioStream>,
    state: Arc<RecordingState>,
}

// SAFETY: AudioStreamManager contains AudioStream which we've marked as Send
unsafe impl Send for AudioStreamManager {}

impl AudioStreamManager {
    pub fn new(state: Arc<RecordingState>) -> Self {
        Self {
            microphone_stream: None,
            system_stream: None,
            state,
        }
    }

    /// Start audio streams for the given devices
    pub async fn start_streams(
        &mut self,
        microphone_device: Option<Arc<AudioDevice>>,
        system_device: Option<Arc<AudioDevice>>,
        recording_sender: Option<mpsc::UnboundedSender<super::recording_state::AudioChunk>>,
    ) -> Result<()> {
        use super::capture::get_current_backend;
        let backend = get_current_backend();
        info!("🎙️ Starting audio streams with backend: {:?}", backend);

        // Start microphone stream
        if let Some(mic_device) = microphone_device {
            info!("🎤 Creating microphone stream: {} (always uses CPAL)", mic_device.name);
            match AudioStream::create(mic_device.clone(), self.state.clone(), DeviceType::Microphone, recording_sender.clone()).await {
                Ok(stream) => {
                    self.state.set_microphone_device(mic_device);
                    self.microphone_stream = Some(stream);
                    info!("✅ Microphone stream created successfully");
                }
                Err(e) => {
                    error!("❌ Failed to create microphone stream: {}", e);
                    return Err(e);
                }
            }
        } else {
            info!("ℹ️ No microphone device specified, skipping microphone stream");
        }

        // Start system audio stream
        if let Some(sys_device) = system_device {
            info!("🔊 Creating system audio stream: {} (backend: {:?})", sys_device.name, backend);
            match AudioStream::create(sys_device.clone(), self.state.clone(), DeviceType::System, recording_sender.clone()).await {
                Ok(stream) => {
                    self.state.set_system_device(sys_device);
                    self.system_stream = Some(stream);
                    info!("✅ System audio stream created with {:?} backend", backend);
                }
                Err(e) => {
                    warn!("⚠️ Failed to create system audio stream: {}", e);
                    // Don't fail if only system audio fails
                }
            }
        } else {
            info!("ℹ️ No system device specified, skipping system audio stream");
        }

        // Ensure at least one stream was created
        if self.microphone_stream.is_none() && self.system_stream.is_none() {
            return Err(anyhow::anyhow!("No audio streams could be created"));
        }

        Ok(())
    }

    /// Stop all audio streams and wait for each capture thread to finish.
    pub async fn stop_streams(&mut self) -> Result<()> {
        info!("Stopping all audio streams");

        let mut errors = Vec::new();

        // Stop microphone stream
        if let Some(mic_stream) = self.microphone_stream.take() {
            if let Err(e) = mic_stream.stop().await {
                error!("Failed to stop microphone stream: {}", e);
                errors.push(e);
            }
        }

        // Stop system stream
        if let Some(sys_stream) = self.system_stream.take() {
            if let Err(e) = sys_stream.stop().await {
                error!("Failed to stop system stream: {}", e);
                errors.push(e);
            }
        }

        if !errors.is_empty() {
            Err(anyhow::anyhow!("Failed to stop some streams: {:?}", errors))
        } else {
            info!("All audio streams stopped successfully");
            Ok(())
        }
    }

    /// Get stream count
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

    /// Check if any streams are active
    pub fn has_active_streams(&self) -> bool {
        self.microphone_stream.is_some() || self.system_stream.is_some()
    }
}

impl Drop for AudioStreamManager {
    fn drop(&mut self) {
        // Drop can't be async, so this is a best-effort signal-only shutdown —
        // it does not wait for capture threads to actually exit. The real,
        // awaited shutdown always happens via an explicit stop_streams().await
        // earlier in every control-flow path (recording_manager.rs); this is
        // only a safety net for paths that drop the manager without calling it.
        if let Some(mic_stream) = self.microphone_stream.take() {
            mic_stream.signal_stop_only();
        }
        if let Some(sys_stream) = self.system_stream.take() {
            sys_stream.signal_stop_only();
        }
    }
}