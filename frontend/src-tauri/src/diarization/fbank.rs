//! Kaldi-compatible log-mel filterbank, configured like sherpa-onnx's CAM++ front end.
//! 25 ms Povey window, 10 ms shift, pre-emphasis 0.97, DC removal, no dither,
//! snip_edges = false, 512-point power spectrum, 80 mel bins from 20 Hz to
//! Nyquist - 400 Hz (7600 Hz at 16 kHz).
use realfft::{RealFftPlanner, RealToComplex};
use std::sync::Arc;

pub const NUM_MEL_BINS: usize = 80;
const PREEMPH: f32 = 0.97;
const LOW_FREQ: f64 = 20.0;
/// As in sherpa-onnx's FeatureExtractorConfig; values <= 0 are an offset from Nyquist.
const HIGH_FREQ: f64 = -400.0;

fn mel(f: f64) -> f64 {
    1127.0 * (1.0 + f / 700.0).ln()
}

fn padded_len(sample_rate: usize) -> usize {
    (sample_rate * 25 / 1000).next_power_of_two()
}

fn mel_edges(sample_rate: usize, bin: usize) -> (f64, f64, f64) {
    let nyquist = sample_rate as f64 / 2.0;
    let high = if HIGH_FREQ <= 0.0 { nyquist + HIGH_FREQ } else { HIGH_FREQ };
    let (lo, hi) = (mel(LOW_FREQ), mel(high));
    let delta = (hi - lo) / (NUM_MEL_BINS as f64 + 1.0);
    let left = lo + bin as f64 * delta;
    (left, left + delta, left + 2.0 * delta)
}

#[cfg(test)]
pub(crate) fn mel_bin_center_hz(sample_rate: usize, bin: usize) -> f64 {
    let (_, center, _) = mel_edges(sample_rate, bin);
    700.0 * ((center / 1127.0).exp() - 1.0)
}

pub struct Fbank {
    frame_length: usize,
    frame_shift: usize,
    padded: usize,
    window: Vec<f32>,
    /// (first FFT bin, weights) per mel bin.
    banks: Vec<(usize, Vec<f32>)>,
    fft: Arc<dyn RealToComplex<f32>>,
}

impl Fbank {
    pub fn new(sample_rate: usize) -> Self {
        let frame_length = sample_rate * 25 / 1000;
        let frame_shift = sample_rate * 10 / 1000;
        let padded = padded_len(sample_rate);
        let a = 2.0 * std::f64::consts::PI / (frame_length as f64 - 1.0);
        let window = (0..frame_length)
            .map(|i| (0.5 - 0.5 * (a * i as f64).cos()).powf(0.85) as f32)
            .collect();

        let bin_width = sample_rate as f64 / padded as f64;
        let banks = (0..NUM_MEL_BINS)
            .map(|b| {
                let (left, center, right) = mel_edges(sample_rate, b);
                let mut first = None;
                let mut weights = Vec::new();
                for i in 0..padded / 2 {
                    let m = mel(bin_width * i as f64);
                    if m > left && m < right {
                        let w = if m <= center { (m - left) / (center - left) } else { (right - m) / (right - center) };
                        first.get_or_insert(i);
                        weights.push(w as f32);
                    }
                }
                (first.unwrap_or(0), weights)
            })
            .collect();

        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(padded);
        Self { frame_length, frame_shift, padded, window, banks, fft }
    }

    pub fn num_frames(&self, num_samples: usize) -> usize {
        (num_samples + self.frame_shift / 2) / self.frame_shift
    }

    pub fn compute(&self, samples: &[f32]) -> Vec<[f32; NUM_MEL_BINS]> {
        let n = samples.len();
        let num_frames = self.num_frames(n);
        let mut out = Vec::with_capacity(num_frames);
        let mut frame = vec![0f32; self.padded];
        let mut spectrum = self.fft.make_output_vec();
        let mut scratch = self.fft.make_scratch_vec();
        let offset = (self.frame_shift / 2) as i64 - (self.frame_length / 2) as i64;

        for f in 0..num_frames {
            let start = (f * self.frame_shift) as i64 + offset;
            for (i, slot) in frame[..self.frame_length].iter_mut().enumerate() {
                *slot = samples[reflect(start + i as i64, n)];
            }
            let mean = frame[..self.frame_length].iter().sum::<f32>() / self.frame_length as f32;
            frame[..self.frame_length].iter_mut().for_each(|x| *x -= mean);
            for i in (1..self.frame_length).rev() {
                frame[i] -= PREEMPH * frame[i - 1];
            }
            frame[0] -= PREEMPH * frame[0];
            for (x, w) in frame[..self.frame_length].iter_mut().zip(&self.window) {
                *x *= w;
            }
            frame[self.frame_length..].iter_mut().for_each(|x| *x = 0.0);

            self.fft
                .process_with_scratch(&mut frame, &mut spectrum, &mut scratch)
                .expect("buffer sizes come from the planner");

            let mut mels = [0f32; NUM_MEL_BINS];
            for (m, (first, weights)) in self.banks.iter().enumerate() {
                let energy: f32 = weights
                    .iter()
                    .enumerate()
                    .map(|(k, w)| w * spectrum[first + k].norm_sqr())
                    .sum();
                mels[m] = energy.max(f32::EPSILON).ln();
            }
            out.push(mels);
        }
        out
    }
}

/// Kaldi's edge handling: mirror indices that fall outside the signal.
fn reflect(mut i: i64, n: usize) -> usize {
    let n = n as i64;
    while i < 0 || i >= n {
        i = if i < 0 { -i - 1 } else { 2 * n - 1 - i };
    }
    i as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_count_matches_kaldi_without_snip_edges() {
        let fb = Fbank::new(16000);
        assert_eq!(fb.num_frames(16000), 100);
        assert_eq!(fb.num_frames(0), 0);
        assert_eq!(fb.num_frames(79), 0);
        assert_eq!(fb.num_frames(80), 1);
        assert_eq!(fb.compute(&vec![0.0; 16000]).len(), 100);
    }

    #[test]
    fn silence_gives_floor_energy() {
        let fb = Fbank::new(16000);
        let frames = fb.compute(&vec![0.0; 1600]);
        let floor = f32::EPSILON.ln();
        assert!(frames.iter().flatten().all(|&v| (v - floor).abs() < 1e-6));
    }

    #[test]
    fn a_1khz_tone_peaks_in_the_1khz_mel_bin() {
        let fb = Fbank::new(16000);
        let tone: Vec<f32> = (0..16000).map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 16000.0).sin() * 0.5).collect();
        let frame = fb.compute(&tone)[50];
        let peak = frame.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).unwrap().0;
        let expected = (0..NUM_MEL_BINS)
            .min_by(|&a, &b| {
                (mel_bin_center_hz(16000, a) - 1000.0).abs().partial_cmp(&(mel_bin_center_hz(16000, b) - 1000.0).abs()).unwrap()
            })
            .unwrap();
        assert!((peak as i64 - expected as i64).abs() <= 1, "peak {peak} expected {expected}");
    }

    #[test]
    #[ignore = "needs DIARIZATION_REF_DIR with reference.json"]
    fn matches_kaldi_native_fbank_reference() {
        let dir = std::path::PathBuf::from(std::env::var("DIARIZATION_REF_DIR").expect("DIARIZATION_REF_DIR"));
        let reference: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("reference.json")).unwrap()).unwrap();
        let file = reference["fbank"]["file"].as_str().unwrap();
        let samples = crate::audio::decoder::decode_audio_file(&dir.join(file)).unwrap().to_whisper_format();
        let frames = Fbank::new(16000).compute(&samples);
        assert_eq!(frames.len() as u64, reference["fbank"]["num_frames"].as_u64().unwrap());
        let mut worst = 0f32;
        for (i, expected) in reference["fbank"]["frames"].as_array().unwrap().iter().enumerate() {
            for (j, e) in expected.as_array().unwrap().iter().enumerate() {
                let e = e.as_f64().unwrap() as f32;
                let err = (frames[i][j] - e).abs() / e.abs().max(1.0);
                worst = worst.max(err);
            }
        }
        assert!(worst < 1e-3, "worst relative error {worst}");
    }
}
