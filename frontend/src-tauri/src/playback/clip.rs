//! Short WAV clips of a recording, for webviews that cannot play the file itself.
use crate::audio::audio_processing::{audio_to_mono, resample_audio};
use crate::audio::decoder::decode_audio_range;
use crate::audio::encode::pcm16_wav;
use anyhow::Result;
use std::path::Path;

/// Clip sample rate: enough to recognise a voice, small enough for one IPC response.
pub const CLIP_RATE: u32 = 16_000;
/// Longest clip rendered in one call.
pub const MAX_CLIP_SECONDS: f64 = 60.0;

/// 16-bit PCM, mono, 16 kHz WAV.
pub fn wav_16k_mono(samples: &[f32]) -> Vec<u8> {
    pcm16_wav(CLIP_RATE, 1, samples)
}

/// `seconds` (at most MAX_CLIP_SECONDS) of the recording from `start_file_s` (container time, the
/// recording clock), as a mono 16 kHz WAV at the recorded level. Shorter near the end of the
/// file; only the header past it.
pub fn render_clip(path: &Path, start_file_s: f64, seconds: f64) -> Result<Vec<u8>> {
    // `seconds > 0.0` is false for NaN, which renders an empty clip.
    let seconds = if seconds > 0.0 { seconds.min(MAX_CLIP_SECONDS) } else { 0.0 };
    if seconds == 0.0 {
        return Ok(wav_16k_mono(&[]));
    }
    let decoded = decode_audio_range(path, start_file_s.max(0.0), seconds)?;
    let channels = decoded.channels.max(1);
    let frames = decoded.samples.len() / channels as usize;
    if frames == 0 {
        return Ok(wav_16k_mono(&[]));
    }
    let mono = if channels > 1 { audio_to_mono(&decoded.samples, channels) } else { decoded.samples };
    let mut clip = resample_audio(&mono, decoded.sample_rate, CLIP_RATE);
    // The resampler's output can be a few samples off; exact lengths keep consecutive clips aligned.
    let length = (frames as f64 * CLIP_RATE as f64 / decoded.sample_rate as f64).round() as usize;
    clip.resize(length, 0.0);
    Ok(wav_16k_mono(&clip))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::decoder::test_audio::{aac_mp4_from_wav, joined_checkpoints, onset_s, silence_then_tone, write_wav};
    use std::path::{Path, PathBuf};

    fn data_size(wav: &[u8]) -> u32 {
        u32::from_le_bytes(wav[40..44].try_into().unwrap())
    }

    fn samples(wav: &[u8]) -> Vec<i16> {
        wav[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()
    }

    /// RMS of `s` on a 0..1 scale.
    fn rms(s: &[i16]) -> f64 {
        (s.iter().map(|&x| (x as f64 / 32768.0).powi(2)).sum::<f64>() / s.len().max(1) as f64).sqrt()
    }

    /// 10 s at 48 kHz mono: 2 s of silence, then a tone.
    fn tone_wav(dir: &Path) -> PathBuf {
        let path = dir.join("audio.wav");
        write_wav(&path, 48_000, 1, &silence_then_tone(48_000, 2.0, 10.0));
        path
    }

    #[test]
    fn wav_header_is_pcm16_mono_16k() {
        let wav = wav_16k_mono(&[0.0, 0.5, -0.5, 1.5, f32::NAN]);
        assert_eq!(wav.len(), 44 + 10);
        assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 1, "mono");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 16_000);
        assert_eq!(u32::from_le_bytes(wav[28..32].try_into().unwrap()), 32_000, "byte rate");
        assert_eq!(data_size(&wav), 10);
        assert_eq!(samples(&wav), vec![0, 16384, -16384, 32767, 0]);

        // The app's own decoder reads it back.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.wav");
        std::fs::write(&path, &wav).unwrap();
        let decoded = crate::audio::decode_audio_file(&path).unwrap();
        assert_eq!((decoded.sample_rate, decoded.channels, decoded.samples.len()), (16_000, 1, 5));
    }

    #[test]
    fn clip_has_requested_length() {
        let dir = tempfile::tempdir().unwrap();
        let wav = render_clip(&tone_wav(dir.path()), 2.0, 3.0).unwrap();
        assert_eq!(data_size(&wav), 3 * 16_000 * 2);
    }

    #[test]
    fn clip_starts_at_requested_position() {
        let dir = tempfile::tempdir().unwrap();
        let wav_input = tone_wav(dir.path());
        let mp4_input = aac_mp4_from_wav(&wav_input);
        for input in [wav_input, mp4_input] {
            // 1.5–2.5 s: the tone starts at the midpoint (21 ms later in AAC, which keeps its priming).
            let clip = samples(&render_clip(&input, 1.5, 1.0).unwrap());
            assert_eq!(clip.len(), 16_000, "{}", input.display());
            assert!(rms(&clip[..6_400]) < 0.01, "silent before 0.4 s in {}", input.display());
            assert!(rms(&clip[9_600..]) > 0.1, "loud after 0.6 s in {}", input.display());
        }
    }

    #[test]
    fn clip_across_a_checkpoint_boundary_stays_on_the_clock() {
        let dir = tempfile::tempdir().unwrap();
        // A live recording of three checkpoints with a tone from 30.5 s and from 60.5 s of the clock.
        let path = joined_checkpoints(dir.path(), 3, |t| (30.5..31.0).contains(&t) || (60.5..61.0).contains(&t));
        for start in [29.0, 59.0] {
            let clip = samples(&render_clip(&path, start, 2.0).unwrap());
            assert_eq!(clip.len(), 2 * 16_000, "a clip across the boundary at {start} + 1 s keeps its length");
            let as_f32: Vec<f32> = clip.iter().map(|&s| s as f32 / 32768.0).collect();
            let onset = onset_s(&as_f32, 16_000).expect("the tone is in the clip");
            // Container time is the clock plus the first checkpoint's 21 ms of priming. Counting
            // decoded frames would put the tone 37 ms later per boundary crossed (about 1.557 s).
            assert!((1.5..1.54).contains(&onset), "tone at {onset} s into the clip from {start} s");
        }
    }

    #[test]
    fn clip_near_the_end_is_shorter() {
        let dir = tempfile::tempdir().unwrap();
        let wav = render_clip(&tone_wav(dir.path()), 9.5, 2.0).unwrap();
        assert_eq!(data_size(&wav), 8_000 * 2, "only the last half second exists");
    }

    #[test]
    fn clip_past_the_end_is_empty_wav() {
        let dir = tempfile::tempdir().unwrap();
        let wav_input = tone_wav(dir.path());
        let mp4_input = aac_mp4_from_wav(&wav_input);
        for input in [wav_input, mp4_input] {
            let wav = render_clip(&input, 12.0, 2.0).unwrap();
            assert_eq!(wav.len(), 44, "{}", input.display());
            assert_eq!(data_size(&wav), 0);
        }
    }

    #[test]
    fn clip_seconds_are_clamped_to_the_maximum() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("long.wav");
        write_wav(&path, 16_000, 1, &silence_then_tone(16_000, 1.0, 70.0));
        let wav = render_clip(&path, 0.0, 90.0).unwrap();
        assert_eq!(data_size(&wav), (MAX_CLIP_SECONDS as u32) * CLIP_RATE * 2);
        assert_eq!(render_clip(&path, 5.0, -1.0).unwrap().len(), 44);
        assert_eq!(render_clip(&path, 5.0, f64::NAN).unwrap().len(), 44);
    }
}
