// Core Audio implementation for macOS system audio capture

#[cfg(target_os = "macos")]
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use anyhow::Result;
use futures_util::Stream;
use ringbuf::{
    traits::{Consumer, Producer, Split},
    HeapCons, HeapProd, HeapRb,
};
use log::{error, info, warn};

#[cfg(target_os = "macos")]
use cidre::{arc, av, cat, cf, core_audio as ca, os};

/// Waker state for async polling
struct WakerState {
    waker: Option<Waker>,
    has_data: bool,
}

/// Core Audio speaker input using aggregate device + tap
#[cfg(target_os = "macos")]
pub struct CoreAudioCapture {
    tap: ca::TapGuard,
    agg_desc: arc::Retained<cf::DictionaryOf<cf::String, cf::Type>>,
}

/// Core Audio stream that produces audio samples
#[cfg(target_os = "macos")]
pub struct CoreAudioStream {
    consumer: HeapCons<f32>,
    _device: ca::hardware::StartedDevice<ca::AggregateDevice>,
    _ctx: Box<AudioContext>,
    _tap: ca::TapGuard,
    waker_state: Arc<Mutex<WakerState>>,
    current_sample_rate: Arc<AtomicU32>,
}

/// Audio processing context
#[cfg(target_os = "macos")]
struct AudioContext {
    format: arc::R<av::AudioFormat>,
    producer: HeapProd<f32>,
    waker_state: Arc<Mutex<WakerState>>,
    current_sample_rate: Arc<AtomicU32>,
    consecutive_drops: Arc<AtomicU32>,
    should_terminate: Arc<AtomicBool>,
}

#[cfg(target_os = "macos")]
impl CoreAudioCapture {
    /// Create a new Core Audio capture for system audio
    pub fn new() -> Result<Self> {
        info!("🎙️ CoreAudio: Starting Core Audio capture initialization...");

        // Note: Audio Capture permission (NSAudioCaptureUsageDescription) is required for macOS 14.4+
        // The permission dialog is automatically triggered when creating the Core Audio tap.
        // If permission is denied, the tap will return silence (all zeros).

        // Get default output device
        info!("🎙️ CoreAudio: Getting default output device...");
        let output_device = ca::System::default_output_device()
            .map_err(|e| {
                error!("❌ CoreAudio: Failed to get default output device: {:?}", e);
                anyhow::anyhow!("Failed to get default output device: {:?}", e)
            })?;

        info!("✅ CoreAudio: Got default output device");

        let output_uid = output_device.uid()
            .map_err(|e| {
                error!("❌ CoreAudio: Failed to get device UID: {:?}", e);
                anyhow::anyhow!("Failed to get device UID: {:?}", e)
            })?;

        // Get device name for better debugging
        let device_name = output_device.name().unwrap_or_else(|_| cf::String::from_str("Unknown"));
        info!("✅ CoreAudio: Default output device: '{}' (UID: {:?})", device_name, output_uid);

        // IMPORTANT: We do NOT create a sub_device dictionary here
        // When using a tap, the tap provides all the audio we need
        // Including both the tap AND the device creates duplicate audio (echo issue)

        // Create process tap with mono global tap, excluding no processes
        // Note: Mono tap is more reliable for system audio capture on macOS
        info!("🎙️ CoreAudio: Creating process tap (global mono tap)...");
        let tap_desc = ca::TapDesc::with_mono_global_tap_excluding_processes(&cidre::ns::Array::new());
        let tap = tap_desc.create_process_tap()
            .map_err(|e| {
                error!("❌ CoreAudio: Failed to create process tap: {:?}", e);
                anyhow::anyhow!("Failed to create process tap: {:?}", e)
            })?;

        // Get tap information
        let tap_uid = tap.uid().unwrap_or_else(|_| cf::Uuid::new().to_cf_string());
        let tap_asbd = tap.asbd();

        match tap_asbd {
            Ok(asbd) => {
                info!("✅ CoreAudio: Process tap created - UID: {:?}", tap_uid);
                info!("📊 CoreAudio: Tap format - sample_rate: {} Hz, channels: {}",
                      asbd.sample_rate, asbd.channels_per_frame);
            }
            Err(e) => {
                warn!("⚠️ CoreAudio: Tap created but couldn't get format info: {:?}", e);
            }
        }

        // Create sub-tap dictionary
        let sub_tap = cf::DictionaryOf::with_keys_values(
            &[ca::sub_device_keys::uid()],
            &[tap.uid().unwrap().as_type_ref()],
        );

        // Create aggregate device descriptor
        // CRITICAL FIX: Use ONLY the tap, NOT the output device + tap
        // Previous configuration included both sub_device_list (output device) and tap_list (tap of same device)
        // This caused duplicate audio capture, resulting in echo (YouTube audio appeared twice)
        // The tap alone provides all the system audio we need
        let agg_desc = cf::DictionaryOf::with_keys_values(
            &[
                ca::aggregate_device_keys::is_private(),
                ca::aggregate_device_keys::is_stacked(),
                ca::aggregate_device_keys::tap_auto_start(),
                ca::aggregate_device_keys::name(),
                ca::aggregate_device_keys::main_sub_device(),
                ca::aggregate_device_keys::uid(),
                // REMOVED: sub_device_list (was causing duplicate audio)
                ca::aggregate_device_keys::tap_list(),
            ],
            &[
                cf::Boolean::value_true().as_type_ref(),
                cf::Boolean::value_false(),
                cf::Boolean::value_true(),
                cf::str!(c"meetily-audio-tap").as_type_ref(),
                &output_uid,
                &cf::Uuid::new().to_cf_string(),
                // REMOVED: sub_device array (was causing echo)
                &cf::ArrayOf::from_slice(&[sub_tap.as_ref()]),
            ],
        );

        info!("✅ CoreAudio: Aggregate device descriptor created");
        info!("✅ CoreAudio: Core Audio capture initialized successfully!");

        Ok(Self { tap, agg_desc })
    }

    /// Start the audio device and create a stream
    fn start_device(
        &self,
        ctx: &mut Box<AudioContext>,
    ) -> Result<ca::hardware::StartedDevice<ca::AggregateDevice>> {
        extern "C" fn audio_proc(
            device: ca::Device,
            _now: &cat::AudioTimeStamp,
            input_data: &cat::AudioBufList<1>,
            _input_time: &cat::AudioTimeStamp,
            _output_data: &mut cat::AudioBufList<1>,
            _output_time: &cat::AudioTimeStamp,
            ctx: Option<&mut AudioContext>,
        ) -> os::Status {
            let ctx = ctx.unwrap();

            // Check for sample rate changes
            let after = device
                .nominal_sample_rate()
                .unwrap_or(ctx.format.absd().sample_rate) as u32;
            let before = ctx.current_sample_rate.load(Ordering::Acquire);

            if before != after {
                ctx.current_sample_rate.store(after, Ordering::Release);
            }

            // Try to get audio data from the buffer list
            if let Some(view) =
                av::AudioPcmBuf::with_buf_list_no_copy(&ctx.format, input_data, None)
            {
                if let Some(data) = view.data_f32_at(0) {
                    process_audio_data(ctx, data);
                }
            } else if ctx.format.common_format() == av::audio::CommonFormat::PcmF32 {
                // Fallback: manual extraction if AudioPcmBuf fails
                let first_buffer = &input_data.buffers[0];
                let byte_count = first_buffer.data_bytes_size as usize;
                let float_count = byte_count / std::mem::size_of::<f32>();

                if float_count > 0 && first_buffer.data != std::ptr::null_mut() {
                    let data = unsafe {
                        std::slice::from_raw_parts(first_buffer.data as *const f32, float_count)
                    };
                    process_audio_data(ctx, data);
                }
            }

            os::Status::NO_ERR
        }

        // Create aggregate device
        info!("🎙️ CoreAudio: Creating aggregate device...");
        let agg_device = ca::AggregateDevice::with_desc(&self.agg_desc)
            .map_err(|e| {
                error!("❌ CoreAudio: Failed to create aggregate device: {:?}", e);
                anyhow::anyhow!("Failed to create aggregate device: {:?}", e)
            })?;

        info!("✅ CoreAudio: Aggregate device created");

        // Create IO proc ID for audio processing
        info!("🎙️ CoreAudio: Creating IO proc...");
        let proc_id = agg_device.create_io_proc_id(audio_proc, Some(ctx))
            .map_err(|e| {
                error!("❌ CoreAudio: Failed to create IO proc: {:?}", e);
                anyhow::anyhow!("Failed to create IO proc: {:?}", e)
            })?;

        info!("✅ CoreAudio: IO proc created with ID: {:?}", proc_id);

        // Start the device
        info!("🎙️ CoreAudio: Starting audio device...");
        let started_device = ca::device_start(agg_device, Some(proc_id))
            .map_err(|e| {
                error!("❌ CoreAudio: Failed to start device: {:?}", e);
                anyhow::anyhow!("Failed to start device: {:?}", e)
            })?;

        info!("✅ CoreAudio: Audio device started successfully!");

        // Get device sample rate
        let device_ref = started_device.as_ref();
        let sample_rate = device_ref.nominal_sample_rate().unwrap_or(0.0);
        info!("📊 CoreAudio: Aggregate device sample_rate: {} Hz", sample_rate);

        Ok(started_device)
    }

    /// Create a stream from this capture
    pub fn stream(self) -> Result<CoreAudioStream> {
        info!("🎙️ CoreAudio: Creating CoreAudioStream...");

        // Get tap audio format
        let asbd = self.tap.asbd()
            .map_err(|e| {
                error!("❌ CoreAudio: Failed to get tap ASBD: {:?}", e);
                anyhow::anyhow!("Failed to get tap ASBD: {:?}", e)
            })?;

        let format = av::AudioFormat::with_asbd(&asbd)
            .ok_or_else(|| {
                error!("❌ CoreAudio: Failed to create audio format");
                anyhow::anyhow!("Failed to create audio format")
            })?;

        info!("✅ CoreAudio: Tap audio format: {} Hz, {} channels", asbd.sample_rate, asbd.channels_per_frame);

        // Create ring buffer for lock-free audio transfer
        let buffer_size = 1024 * 128; // 128KB buffer
        let rb = HeapRb::<f32>::new(buffer_size);
        let (producer, consumer) = rb.split();

        let waker_state = Arc::new(Mutex::new(WakerState {
            waker: None,
            has_data: false,
        }));

        let current_sample_rate = Arc::new(AtomicU32::new(asbd.sample_rate as u32));
        info!("✅ CoreAudio: Initial sample rate: {} Hz", asbd.sample_rate);

        let mut ctx = Box::new(AudioContext {
            format,
            producer,
            waker_state: waker_state.clone(),
            current_sample_rate: current_sample_rate.clone(),
            consecutive_drops: Arc::new(AtomicU32::new(0)),
            should_terminate: Arc::new(AtomicBool::new(false)),
        });

        info!("🎙️ CoreAudio: Starting audio device...");
        let device = self.start_device(&mut ctx)?;

        info!("✅ CoreAudio: CoreAudioStream created successfully!");

        Ok(CoreAudioStream {
            consumer,
            _device: device,
            _ctx: ctx,
            _tap: self.tap,
            waker_state,
            current_sample_rate,
        })
    }
}

/// Process audio data from the IO proc callback
#[cfg(target_os = "macos")]
fn process_audio_data(ctx: &mut AudioContext, data: &[f32]) {
    // Push raw samples directly to ring buffer
    // Let the pipeline handle all gain adjustments (post-mix 3x gain + mic normalization)
    let buffer_size = data.len();
    let pushed = ctx.producer.push_slice(data);

    if pushed < buffer_size {
        let consecutive = ctx.consecutive_drops.fetch_add(1, Ordering::AcqRel) + 1;

        if consecutive > 10 {
            ctx.should_terminate.store(true, Ordering::Release);
            return;
        }
    } else {
        ctx.consecutive_drops.store(0, Ordering::Release);
    }

    if pushed > 0 {
        let should_wake = {
            let mut waker_state = ctx.waker_state.lock().unwrap();
            if !waker_state.has_data {
                waker_state.has_data = true;
                waker_state.waker.take()
            } else {
                None
            }
        };

        if let Some(waker) = should_wake {
            waker.wake();
        }
    }
}

#[cfg(target_os = "macos")]
impl CoreAudioStream {
    /// Get current sample rate
    pub fn sample_rate(&self) -> u32 {
        self.current_sample_rate.load(Ordering::Acquire)
    }
}

#[cfg(target_os = "macos")]
impl Stream for CoreAudioStream {
    type Item = f32;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        // Try to pop a sample from the ring buffer
        if let Some(sample) = self.consumer.try_pop() {
            return Poll::Ready(Some(sample));
        }

        // Check if we should terminate
        if self._ctx.should_terminate.load(Ordering::Acquire) {
            warn!("Stream terminating due to buffer pressure");
            return match self.consumer.try_pop() {
                Some(sample) => Poll::Ready(Some(sample)),
                None => Poll::Ready(None),
            };
        }

        // No data available, register waker and return pending
        {
            let mut state = self.waker_state.lock().unwrap();
            state.has_data = false;
            state.waker = Some(cx.waker().clone());
        }

        Poll::Pending
    }
}

#[cfg(target_os = "macos")]
impl Drop for CoreAudioStream {
    fn drop(&mut self) {
        info!("CoreAudioStream dropped, signaling termination");
        self._ctx.should_terminate.store(true, Ordering::Release);
    }
}

// Stub implementations for non-macOS platforms
#[cfg(not(target_os = "macos"))]
pub struct CoreAudioCapture;

#[cfg(not(target_os = "macos"))]
pub struct CoreAudioStream;

#[cfg(not(target_os = "macos"))]
impl CoreAudioCapture {
    pub fn new() -> Result<Self> {
        Err(anyhow::anyhow!("Core Audio is only supported on macOS"))
    }

    pub fn stream(self) -> Result<CoreAudioStream> {
        Err(anyhow::anyhow!("Core Audio is only supported on macOS"))
    }
}

#[cfg(not(target_os = "macos"))]
impl CoreAudioStream {
    pub fn sample_rate(&self) -> u32 {
        0
    }
}

#[cfg(not(target_os = "macos"))]
impl Stream for CoreAudioStream {
    type Item = f32;

    fn poll_next(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        Poll::Ready(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[cfg(target_os = "macos")]
    #[ignore] // Only run manually as it requires audio hardware
    async fn test_core_audio_capture() {
        use futures_util::StreamExt;

        let capture = CoreAudioCapture::new().expect("Failed to create capture");
        let mut stream = capture.stream().expect("Failed to create stream");

        info!("Stream sample rate: {} Hz", stream.sample_rate());

        // Collect some samples
        let mut sample_count = 0;
        while sample_count < 48000 { // 1 second at 48kHz
            if let Some(_sample) = stream.next().await {
                sample_count += 1;
            }
        }

        info!("Collected {} samples", sample_count);
        assert!(sample_count >= 48000);
    }
}