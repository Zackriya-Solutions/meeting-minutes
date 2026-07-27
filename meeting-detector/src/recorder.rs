//! Mic + system-audio capture, mixed to a crash-safe WAV file.
//!
//! Two independent sources feed one mixer, matching the main app:
//! - **mic** via cpal (its realtime callback sends chunks over a channel), and
//! - **system audio** via the CoreAudio tap (`system_audio`), drained on a worker
//!   thread with `block_on_stream` (no async runtime needed).
//!
//! The mixer thread resamples each stream to 48 kHz, aligns them in a ring buffer,
//! mixes (clamp of the sum), and writes mono 16-bit PCM — flushed ~once per second
//! so a crash mid-call still leaves a playable file. If system capture is
//! unavailable (pre-14.4, permission denied, no output device) it degrades to
//! mic-only.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;

use crate::mixer::{AudioMixerRingBuffer, LinearResampler, ProfessionalAudioMixer, Track};
use crate::system_audio::{CoreAudioCapture, CoreAudioStream};

/// Common rate all streams are resampled to before mixing (matches the app).
const MIX_RATE: u32 = 48_000;

/// Result of a finished capture, consumed by the registration step.
pub struct RecordingInfo {
    pub wav_path: PathBuf,
    pub duration_secs: f64,
    pub device_name: String,
}

/// A raw (native-rate) mono chunk tagged with its source.
enum MixMsg {
    Mic(Vec<f32>),
    System(Vec<f32>),
}

pub struct Recorder {
    mic_stream: cpal::Stream,
    system_stop: Option<Arc<AtomicBool>>,
    system_thread: Option<JoinHandle<()>>,
    mixer_thread: JoinHandle<Result<u64>>,
    wav_path: PathBuf,
    device_name: String,
}

impl Recorder {
    /// Start capturing mic + system audio, mixing into `wav_path`.
    pub fn start(wav_path: PathBuf) -> Result<Self> {
        // --- Microphone (cpal) ---
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("no default input (microphone) device"))?;
        let device_name = device.name().unwrap_or_else(|_| "microphone".to_string());
        let default_config = device
            .default_input_config()
            .context("querying default input config")?;
        let sample_format = default_config.sample_format();
        let config: cpal::StreamConfig = default_config.into();
        let mic_rate = config.sample_rate.0;
        let mic_channels = config.channels as usize;

        // --- System audio (CoreAudio tap), optional ---
        let system = try_start_system();
        let sys_rate = system.as_ref().map(|(_, rate)| *rate);

        log::info!(
            "recording mic '{device_name}' @ {mic_rate} Hz ({mic_channels} ch, {sample_format:?}); \
             system audio: {}",
            match sys_rate {
                Some(r) => format!("on @ {r} Hz"),
                None => "off (mic only)".to_string(),
            }
        );

        // --- Mixer thread ---
        let (tx, rx): (Sender<MixMsg>, Receiver<MixMsg>) = std::sync::mpsc::channel();
        let wav_for_thread = wav_path.clone();
        let mixer_thread = std::thread::Builder::new()
            .name("mixer".into())
            .spawn(move || mix_loop(rx, wav_for_thread, mic_rate, sys_rate))
            .context("spawning mixer thread")?;

        // --- Mic stream: callback downmixes to mono f32 and sends chunks ---
        let mic_tx = tx.clone();
        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                &config,
                move |data: &[f32], _: &_| {
                    let _ = mic_tx.send(MixMsg::Mic(downmix_f32(data, mic_channels)));
                },
                on_stream_error,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                &config,
                move |data: &[i16], _: &_| {
                    let _ = mic_tx.send(MixMsg::Mic(downmix_i16(data, mic_channels)));
                },
                on_stream_error,
                None,
            ),
            SampleFormat::U16 => device.build_input_stream(
                &config,
                move |data: &[u16], _: &_| {
                    let _ = mic_tx.send(MixMsg::Mic(downmix_u16(data, mic_channels)));
                },
                on_stream_error,
                None,
            ),
            SampleFormat::I32 => device.build_input_stream(
                &config,
                move |data: &[i32], _: &_| {
                    let _ = mic_tx.send(MixMsg::Mic(downmix_i32(data, mic_channels)));
                },
                on_stream_error,
                None,
            ),
            other => return Err(anyhow!("unsupported input sample format: {other:?}")),
        }
        .context("building mic input stream")?;
        stream.play().context("starting mic stream")?;

        // --- System drain thread (if system capture started) ---
        let (system_stop, system_thread) = match system {
            Some((sys_stream, _)) => {
                let stop = Arc::new(AtomicBool::new(false));
                let handle = spawn_system_drain(sys_stream, tx.clone(), stop.clone());
                (Some(stop), Some(handle))
            }
            None => (None, None),
        };

        // Drop the original sender so the mixer's channel closes once the mic
        // callback and system thread (the only remaining clones) are gone.
        drop(tx);

        Ok(Self {
            mic_stream: stream,
            system_stop,
            system_thread,
            mixer_thread,
            wav_path,
            device_name,
        })
    }

    /// Stop capturing and finalize the WAV, returning info about the recording.
    pub fn stop(self) -> Result<RecordingInfo> {
        let Recorder {
            mic_stream,
            system_stop,
            system_thread,
            mixer_thread,
            wav_path,
            device_name,
        } = self;

        // Stop mic capture (drops the callback and its channel sender).
        drop(mic_stream);
        // Stop system capture (thread breaks its loop, drops the stream + sender).
        if let Some(stop) = system_stop {
            stop.store(true, Ordering::Relaxed);
        }
        if let Some(handle) = system_thread {
            let _ = handle.join();
        }

        // Both senders are gone now -> mixer channel closes -> WAV finalized.
        let frames = mixer_thread
            .join()
            .map_err(|_| anyhow!("mixer thread panicked"))??;
        let duration_secs = frames as f64 / MIX_RATE as f64;

        Ok(RecordingInfo {
            wav_path,
            duration_secs,
            device_name,
        })
    }
}

/// Try to start system-audio capture; returns the stream + its sample rate, or
/// `None` (logged) so recording can proceed mic-only.
fn try_start_system() -> Option<(CoreAudioStream, u32)> {
    match CoreAudioCapture::new().and_then(|capture| capture.stream()) {
        Ok(stream) => {
            let rate = stream.sample_rate();
            Some((stream, rate))
        }
        Err(e) => {
            log::warn!("system-audio capture unavailable ({e:#}); recording mic only");
            None
        }
    }
}

/// Drain the CoreAudio stream on a dedicated thread, batching samples to the mixer.
fn spawn_system_drain(
    stream: CoreAudioStream,
    tx: Sender<MixMsg>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("system-audio".into())
        .spawn(move || {
            let mut iter = futures_executor::block_on_stream(stream);
            let mut batch: Vec<f32> = Vec::with_capacity(2048);
            while !stop.load(Ordering::Relaxed) {
                match iter.next() {
                    Some(sample) => {
                        batch.push(sample);
                        if batch.len() >= 1024 {
                            let chunk = std::mem::replace(&mut batch, Vec::with_capacity(2048));
                            if tx.send(MixMsg::System(chunk)).is_err() {
                                break;
                            }
                        }
                    }
                    None => break,
                }
            }
            if !batch.is_empty() {
                let _ = tx.send(MixMsg::System(batch));
            }
            // `iter` (and the CoreAudio stream) dropped here -> tap stops.
        })
        .expect("spawning system-audio thread")
}

/// Mixer thread: resample each source to 48 kHz, align, mix, write WAV.
fn mix_loop(
    rx: Receiver<MixMsg>,
    wav_path: PathBuf,
    mic_rate: u32,
    sys_rate: Option<u32>,
) -> Result<u64> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: MIX_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut wav = hound::WavWriter::create(&wav_path, spec).context("creating WAV file")?;

    let mut mic_resampler = LinearResampler::new(mic_rate, MIX_RATE);
    let mut sys_resampler = LinearResampler::new(sys_rate.unwrap_or(MIX_RATE), MIX_RATE);
    let mut ring = AudioMixerRingBuffer::new(MIX_RATE);
    let mut mixer = ProfessionalAudioMixer::new();

    let mut frames: u64 = 0;
    let mut since_flush: u64 = 0;

    let write_window = |wav: &mut hound::WavWriter<_>,
                            frames: &mut u64,
                            since_flush: &mut u64,
                            mixed: &[f32]|
     -> Result<()> {
        for &sample in mixed {
            wav.write_sample(f32_to_i16(sample))?;
        }
        *frames += mixed.len() as u64;
        *since_flush += mixed.len() as u64;
        if *since_flush >= MIX_RATE as u64 {
            wav.flush()?;
            *since_flush = 0;
        }
        Ok(())
    };

    while let Ok(msg) = rx.recv() {
        match msg {
            MixMsg::Mic(chunk) => {
                let resampled = mic_resampler.process(&chunk);
                ring.add_samples(Track::Mic, &resampled);
            }
            MixMsg::System(chunk) => {
                let resampled = sys_resampler.process(&chunk);
                ring.add_samples(Track::System, &resampled);
            }
        }
        while ring.can_mix() {
            if let Some((mic_window, sys_window)) = ring.extract_window() {
                let mixed = mixer.mix_window(&mic_window, &sys_window);
                write_window(&mut wav, &mut frames, &mut since_flush, &mixed)?;
            }
        }
    }

    // Flush the final partial window.
    if let Some((mic_window, sys_window)) = ring.flush() {
        let mixed = mixer.mix_window(&mic_window, &sys_window);
        write_window(&mut wav, &mut frames, &mut since_flush, &mixed)?;
    }

    wav.finalize()?;
    Ok(frames)
}

fn on_stream_error(err: cpal::StreamError) {
    log::error!("mic stream error: {err}");
}

fn f32_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

fn downmix_f32(data: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return data.to_vec();
    }
    data.chunks(channels)
        .map(|frame| frame.iter().copied().sum::<f32>() / channels as f32)
        .collect()
}

fn downmix_i16(data: &[i16], channels: usize) -> Vec<f32> {
    let norm = |s: i16| s as f32 / 32768.0;
    if channels <= 1 {
        return data.iter().map(|&s| norm(s)).collect();
    }
    data.chunks(channels)
        .map(|frame| frame.iter().map(|&s| norm(s)).sum::<f32>() / channels as f32)
        .collect()
}

fn downmix_u16(data: &[u16], channels: usize) -> Vec<f32> {
    let norm = |s: u16| (s as f32 - 32768.0) / 32768.0;
    if channels <= 1 {
        return data.iter().map(|&s| norm(s)).collect();
    }
    data.chunks(channels)
        .map(|frame| frame.iter().map(|&s| norm(s)).sum::<f32>() / channels as f32)
        .collect()
}

fn downmix_i32(data: &[i32], channels: usize) -> Vec<f32> {
    let norm = |s: i32| s as f32 / 2_147_483_648.0;
    if channels <= 1 {
        return data.iter().map(|&s| norm(s)).collect();
    }
    data.chunks(channels)
        .map(|frame| frame.iter().map(|&s| norm(s)).sum::<f32>() / channels as f32)
        .collect()
}
