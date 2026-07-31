//! Mic + system-audio capture for background auto-recording, written straight to
//! disk with no transcription attached.
//!
//! This reuses the normal recording path's building blocks rather than opening its
//! own devices: [`AudioStreamManager`] creates the CPAL microphone stream and the
//! platform system-audio stream, each wrapped in the same `AudioCapture` that
//! downmixes to mono, resamples to 48 kHz, and conditions the microphone (high-pass
//! → RNNoise → EBU R128). The difference from `audio::pipeline` is what happens
//! next: chunks are mixed and handed to [`IncrementalAudioSaver`], never to the VAD
//! or a transcription engine.
//!
//! The capture owns a *private* [`RecordingState`], so it is completely independent
//! of the interactive recording session — `is_recording()` stays false, no
//! transcript events are emitted, and the UI is never navigated.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use log::{debug, info, warn};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::audio::incremental_saver::IncrementalAudioSaver;
use crate::audio::pipeline::{AudioMixerRingBuffer, ProfessionalAudioMixer};
use crate::audio::recording_state::{AudioChunk, DeviceType, RecordingState};
use crate::audio::stream::AudioStreamManager;

#[cfg(target_os = "macos")]
use crate::audio::devices::get_safe_recording_devices_macos;
#[cfg(not(target_os = "macos"))]
use crate::audio::devices::{default_input_device, default_output_device};

/// Every stream is resampled to this rate before mixing (matches the live path).
const MIX_RATE: u32 = 48_000;

/// What a finished background capture produced.
pub struct BackgroundRecording {
    /// Final merged `audio.mp4` inside the meeting folder.
    pub audio_path: PathBuf,
    pub duration_secs: f64,
    pub microphone: Option<String>,
    pub system_audio: Option<String>,
}

pub struct BackgroundRecorder {
    state: Arc<RecordingState>,
    streams: AudioStreamManager,
    /// Mixes captured chunks and feeds the incremental saver until the channel
    /// closes; yields the saver back so it can be finalized after the streams stop.
    mixer: JoinHandle<Result<(IncrementalAudioSaver, u64)>>,
    microphone: Option<String>,
    system_audio: Option<String>,
}

impl BackgroundRecorder {
    /// Start capturing into `meeting_folder`, which must already contain a
    /// `.checkpoints/` directory (see `create_meeting_folder(.., true)`).
    pub async fn start(meeting_folder: PathBuf) -> Result<Self> {
        let (microphone_device, system_device) = select_devices()?;
        let microphone_device =
            microphone_device.ok_or_else(|| anyhow!("no microphone device available"))?;

        let microphone = Some(microphone_device.name.clone());
        let system_audio = system_device.as_ref().map(|device| device.name.clone());

        // A private state object: the interactive session's recording state, and
        // therefore the whole transcription pipeline, is left untouched.
        let state = RecordingState::new();
        let (chunk_sender, chunk_receiver) = mpsc::unbounded_channel::<AudioChunk>();
        state.set_audio_sender(chunk_sender);
        state.start_recording()?;

        let saver = IncrementalAudioSaver::new(meeting_folder, MIX_RATE)
            .context("initializing incremental saver for background capture")?;

        // Mixing runs on a blocking thread: each 30 s checkpoint shells out to
        // ffmpeg, which must not stall an async worker.
        let mixer = tokio::task::spawn_blocking(move || mix_loop(chunk_receiver, saver));

        let mut streams = AudioStreamManager::new(state.clone());
        if let Err(error) = streams
            .start_streams(
                Some(microphone_device),
                system_device,
                None, // mixed audio is saved here, not through the recording saver
            )
            .await
        {
            state.stop_recording();
            state.cleanup();
            let _ = mixer.await;
            return Err(error).context("starting background capture streams");
        }

        info!(
            "🎙️ Background capture started (mic: {:?}, system: {:?})",
            microphone, system_audio
        );

        Ok(Self {
            state,
            streams,
            mixer,
            microphone,
            system_audio,
        })
    }

    /// Stop the streams, flush the tail, and merge the checkpoints into `audio.mp4`.
    pub async fn stop(mut self) -> Result<BackgroundRecording> {
        // Order matters: silence the streams first so no chunk is produced after the
        // channel closes, then clear the sender. Doing it the other way round drops
        // the in-flight callbacks' last chunks and logs them as pipeline errors.
        if let Err(error) = self.streams.stop_streams() {
            warn!("Background capture streams did not stop cleanly: {error}");
        }
        self.state.stop_recording();
        // Drops the chunk sender, which ends the mix loop.
        self.state.cleanup();

        let (mut saver, frames) = self
            .mixer
            .await
            .map_err(|error| anyhow!("background capture mixer failed: {error}"))??;

        let duration_secs = frames as f64 / MIX_RATE as f64;
        let audio_path = saver
            .finalize()
            .await
            .context("merging background capture audio")?;

        info!(
            "✅ Background capture stopped: {:.1}s of audio at {}",
            duration_secs,
            audio_path.display()
        );

        Ok(BackgroundRecording {
            audio_path,
            duration_secs,
            microphone: self.microphone,
            system_audio: self.system_audio,
        })
    }
}

/// Pick the same devices the interactive recorder would use, including the macOS
/// Bluetooth→built-in override that keeps sample rates stable while mixing.
type SelectedDevices = (
    Option<Arc<crate::audio::devices::AudioDevice>>,
    Option<Arc<crate::audio::devices::AudioDevice>>,
);

fn select_devices() -> Result<SelectedDevices> {
    #[cfg(target_os = "macos")]
    {
        let (microphone, system) = get_safe_recording_devices_macos()?;
        Ok((microphone.map(Arc::new), system.map(Arc::new)))
    }

    #[cfg(not(target_os = "macos"))]
    {
        let microphone = default_input_device().ok().map(Arc::new);
        let system = match default_output_device() {
            Ok(device) => Some(Arc::new(device)),
            Err(error) => {
                warn!("Background capture has no system audio device: {error}");
                None
            }
        };
        Ok((microphone, system))
    }
}

/// Align mic/system chunks into fixed windows, mix them, and append to the saver.
/// Returns the saver plus the number of mixed frames written.
fn mix_loop(
    mut chunks: mpsc::UnboundedReceiver<AudioChunk>,
    mut saver: IncrementalAudioSaver,
) -> Result<(IncrementalAudioSaver, u64)> {
    let mut ring = AudioMixerRingBuffer::new(MIX_RATE);
    let mut mixer = ProfessionalAudioMixer::new(MIX_RATE);
    let mut frames: u64 = 0;

    while let Some(chunk) = chunks.blocking_recv() {
        let device_type = chunk.device_type.clone();
        ring.add_samples(device_type, chunk.data);

        while ring.can_mix() {
            let Some((mic_window, system_window)) = ring.extract_window() else {
                break;
            };
            frames += write_window(&mut saver, &mut mixer, &mic_window, &system_window)?;
        }
    }

    // The last partial window would otherwise be lost.
    if let Some((mic_window, system_window)) = ring.flush() {
        frames += write_window(&mut saver, &mut mixer, &mic_window, &system_window)?;
    }

    debug!("Background capture mix loop ended after {frames} mixed frames");
    Ok((saver, frames))
}

fn write_window(
    saver: &mut IncrementalAudioSaver,
    mixer: &mut ProfessionalAudioMixer,
    mic_window: &[f32],
    system_window: &[f32],
) -> Result<u64> {
    let mixed = mixer.mix_window(mic_window, system_window);
    let written = mixed.len() as u64;

    saver.add_chunk(AudioChunk {
        data: mixed,
        sample_rate: MIX_RATE,
        timestamp: 0.0,
        chunk_id: 0,
        device_type: DeviceType::Microphone, // the saver stores mixed audio only
        speaker: None,
    })?;

    Ok(written)
}
