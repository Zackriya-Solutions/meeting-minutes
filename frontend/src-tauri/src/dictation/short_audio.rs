use crate::audio::audio_processing::{audio_to_mono, resample_audio};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use std::fmt;
use std::sync::{Arc, Mutex};

const TRANSCRIPTION_SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortAudioError {
    NoMicrophone,
    DeviceConfig(String),
    UnsupportedFormat(String),
    Start(String),
    BufferUnavailable,
    TooShort,
}

impl fmt::Display for ShortAudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoMicrophone => write!(f, "no default microphone is available"),
            Self::DeviceConfig(message) => write!(f, "microphone configuration failed: {message}"),
            Self::UnsupportedFormat(format) => {
                write!(f, "microphone sample format is unsupported: {format}")
            }
            Self::Start(message) => write!(f, "microphone stream failed to start: {message}"),
            Self::BufferUnavailable => write!(f, "microphone sample buffer is unavailable"),
            Self::TooShort => write!(f, "dictation was too short to transcribe"),
        }
    }
}

impl std::error::Error for ShortAudioError {}

pub struct ShortAudioCapture {
    stream: Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
}

impl ShortAudioCapture {
    pub fn start() -> Result<Self, ShortAudioError> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(ShortAudioError::NoMicrophone)?;
        let supported = device
            .default_input_config()
            .map_err(|error| ShortAudioError::DeviceConfig(error.to_string()))?;
        let sample_rate = supported.sample_rate().0;
        let channels = supported.channels();
        let config: StreamConfig = supported.clone().into();
        let samples = Arc::new(Mutex::new(Vec::with_capacity(sample_rate as usize * 30)));

        let stream = match supported.sample_format() {
            SampleFormat::F32 => {
                build_stream::<f32>(&device, &config, samples.clone(), |sample| sample)
            }
            SampleFormat::I16 => build_stream::<i16>(&device, &config, samples.clone(), |sample| {
                sample as f32 / i16::MAX as f32
            }),
            SampleFormat::U16 => build_stream::<u16>(&device, &config, samples.clone(), |sample| {
                (sample as f32 / u16::MAX as f32) * 2.0 - 1.0
            }),
            format => return Err(ShortAudioError::UnsupportedFormat(format.to_string())),
        }
        .map_err(|error| ShortAudioError::Start(error.to_string()))?;

        stream
            .play()
            .map_err(|error| ShortAudioError::Start(error.to_string()))?;

        Ok(Self {
            stream,
            samples,
            sample_rate,
            channels,
        })
    }

    pub fn finish(self) -> Result<Vec<f32>, ShortAudioError> {
        let _ = self.stream.pause();
        drop(self.stream);
        let samples = self
            .samples
            .lock()
            .map_err(|_| ShortAudioError::BufferUnavailable)?
            .clone();
        prepare_samples(&samples, self.channels, self.sample_rate)
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    convert: fn(T) -> f32,
) -> Result<Stream, cpal::BuildStreamError>
where
    T: cpal::SizedSample + Copy + 'static,
{
    device.build_input_stream(
        config,
        move |data: &[T], _| {
            if let Ok(mut buffer) = samples.lock() {
                buffer.extend(data.iter().copied().map(convert));
            }
        },
        |error| {
            log::error!("dictation_audio_stream_failed code=audio_capture_failed error={error}")
        },
        None,
    )
}

fn prepare_samples(
    interleaved: &[f32],
    channels: u16,
    sample_rate: u32,
) -> Result<Vec<f32>, ShortAudioError> {
    let mono = audio_to_mono(interleaved, channels);
    let prepared = if sample_rate == TRANSCRIPTION_SAMPLE_RATE {
        mono
    } else {
        resample_audio(&mono, sample_rate, TRANSCRIPTION_SAMPLE_RATE)
    };
    if prepared.len() < (TRANSCRIPTION_SAMPLE_RATE / 5) as usize {
        return Err(ShortAudioError::TooShort);
    }
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_stereo_to_mono_for_transcription() {
        let mut stereo = Vec::with_capacity(6_400);
        for _ in 0..3_200 {
            stereo.extend_from_slice(&[0.5, -0.5]);
        }

        let prepared = prepare_samples(&stereo, 2, 16_000).unwrap();

        assert_eq!(prepared.len(), 3_200);
        assert!(prepared.iter().all(|sample| sample.abs() < f32::EPSILON));
    }

    #[test]
    fn rejects_accidental_taps() {
        let error = prepare_samples(&vec![0.0; 1_000], 1, 16_000).unwrap_err();
        assert_eq!(error, ShortAudioError::TooShort);
    }
}
