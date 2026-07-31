//! Kaldi-compatible 80-dim log-mel filterbank for the WeSpeaker speaker-embedding
//! model (PLAN.md Phase 2 diarization).
//!
//! The WeSpeaker CAM++ / ResNet34 ONNX exports (from wespeaker / sherpa-onnx) expect
//! `feats` of shape `[1, num_frames, 80]`: kaldi `fbank` features.
//!
//! Frame/mel config matches `kaldi-native-fbank` (as used by sherpa-onnx and
//! `pyannote-rs`' `knf-rs` wrapper):
//!   frame_opts: samp_freq=16000, frame_length=25ms (400), frame_shift=10ms (160),
//!               dither=0, snip_edges=true, preemph=0.97, remove_dc_offset=true,
//!               window="povey", round_to_power_of_two=true (=> 512-point FFT)
//!   mel_opts:   num_bins=80, low_freq=20, high_freq=8000 (Nyquist), use_power=true,
//!               use_log_fbank=true, use_energy=false
//!
//! ## Normalization: follow the MODEL's ONNX metadata, not knf-rs
//! sherpa-onnx reads two metadata keys from each speaker-embedding ONNX and adapts the
//! frontend (`speaker-embedding-extractor-general-impl.h` + `features.cc`):
//!   * `normalize_samples` — `0` means the model wants **i16-scale** waveforms (samples
//!     multiplied by 32768; WeSpeaker trains on `torchaudio.load(normalize=False)` audio).
//!     Without CMN the scale matters enormously: log-mels live around [5, 25] at i16 scale
//!     vs [-16, 4] at [-1, 1] scale.
//!   * `feature_normalize_type` — CMN (`global-mean`) is applied ONLY when this key exists.
//!
//! `wespeaker_en_voxceleb_CAM++.onnx` (meetily's embedding model) declares
//! `normalize_samples=0` and **no** `feature_normalize_type`: raw i16-scale fbank, NO CMN.
//! `knf-rs` unconditionally feeds [-1, 1] samples and subtracts the per-utterance mean;
//! measured on real speech (CMU Arctic 3-speaker probe) that combination makes the
//! embeddings nearly speaker-agnostic (same-speaker cosine ≈ cross-speaker ≈ 0.6, with
//! pairs as wild as same=0.07 / cross=0.97) — the direct cause of a real 3-person meeting
//! diarizing as 15 speakers. [`KaldiFbank::compute_raw`] (no CMN) + i16-scale input
//! restores real speaker discrimination; [`KaldiFbank::compute`] (with CMN) remains for
//! models whose metadata asks for `global-mean`.

use std::sync::Arc;

use realfft::{RealFftPlanner, RealToComplex};

pub const SAMPLE_RATE: usize = 16_000;
pub const FRAME_LENGTH: usize = 400; // 25 ms @ 16 kHz
pub const FRAME_SHIFT: usize = 160; // 10 ms @ 16 kHz
pub const N_FFT: usize = 512; // round_up_to_pow2(400)
pub const N_FREQ: usize = N_FFT / 2 + 1; // 257
pub const N_FFT_BINS: usize = N_FFT / 2; // 256 (mel filters ignore the Nyquist bin, per kaldi)
pub const N_MELS: usize = 80;
const LOW_FREQ: f32 = 20.0;
const HIGH_FREQ: f32 = 8000.0; // Nyquist
const PREEMPH_COEFF: f32 = 0.97;

fn mel_scale(freq: f32) -> f32 {
    1127.0 * (1.0 + freq / 700.0).ln()
}

/// Kaldi-compatible fbank featurizer. Precomputes the Povey window and the triangular
/// mel filterbank once (mirrors [`crate::gigaam_engine::featurizer::Featurizer`]).
pub struct KaldiFbank {
    window: Vec<f32>, // [FRAME_LENGTH] Povey window
    fbank: Vec<f32>,  // [N_MELS * N_FFT_BINS] row-major: fbank[m * N_FFT_BINS + k]
    fft: Arc<dyn RealToComplex<f32>>,
}

impl KaldiFbank {
    pub fn new() -> Self {
        // Povey window: (0.5 - 0.5*cos(2*pi*i/(N-1)))^0.85
        let n = FRAME_LENGTH;
        let a = 2.0 * std::f32::consts::PI / (n as f32 - 1.0);
        let window: Vec<f32> = (0..n)
            .map(|i| (0.5 - 0.5 * (a * i as f32).cos()).powf(0.85))
            .collect();

        // Triangular mel filterbank over fft bins 0..N_FFT_BINS-1 (kaldi ignores Nyquist).
        // high_freq stays at the kaldi/torchaudio default (Nyquist) — WeSpeaker trains with
        // torchaudio kaldi fbank defaults; sherpa-onnx's 7600 Hz measured slightly worse
        // (same/cross clip-mean gap 0.134 vs 0.175 on the CMU Arctic probe).
        let fft_bin_width = SAMPLE_RATE as f32 / N_FFT as f32; // 31.25 Hz
        let mel_low = mel_scale(LOW_FREQ);
        let mel_high = mel_scale(HIGH_FREQ);
        let mel_delta = (mel_high - mel_low) / (N_MELS as f32 + 1.0);

        let mut fbank = vec![0f32; N_MELS * N_FFT_BINS];
        for m in 0..N_MELS {
            let left_mel = mel_low + m as f32 * mel_delta;
            let center_mel = mel_low + (m as f32 + 1.0) * mel_delta;
            let right_mel = mel_low + (m as f32 + 2.0) * mel_delta;
            for k in 0..N_FFT_BINS {
                let freq = fft_bin_width * k as f32;
                let mel = mel_scale(freq);
                if mel > left_mel && mel < right_mel {
                    let weight = if mel <= center_mel {
                        (mel - left_mel) / (center_mel - left_mel)
                    } else {
                        (right_mel - mel) / (right_mel - center_mel)
                    };
                    fbank[m * N_FFT_BINS + k] = weight;
                }
            }
        }

        let mut planner = RealFftPlanner::<f32>::new();
        Self {
            window,
            fbank,
            fft: planner.plan_fft_forward(N_FFT),
        }
    }

    /// Number of frames for `n_samples` with `snip_edges=true` (kaldi semantics):
    /// `0` if shorter than one frame, else `1 + (n_samples - FRAME_LENGTH) / FRAME_SHIFT`.
    pub fn num_frames(n_samples: usize) -> usize {
        if n_samples < FRAME_LENGTH {
            0
        } else {
            1 + (n_samples - FRAME_LENGTH) / FRAME_SHIFT
        }
    }

    /// Compute CMN-normalized kaldi fbank for a 16 kHz mono waveform (for models whose
    /// metadata declares `feature_normalize_type=global-mean`, e.g. 3D-Speaker CAM++).
    /// Returns `[num_frames, N_MELS]` (row-major) as an ndarray, ready to gain a batch axis.
    pub fn compute(&self, waveform: &[f32]) -> ndarray::Array2<f32> {
        let feats = self.compute_raw(waveform);
        // CMN: subtract the per-bin mean over time (sherpa's "global-mean").
        if feats.shape()[0] == 0 {
            return feats;
        }
        if let Some(mean) = feats.mean_axis(ndarray::Axis(0)) {
            feats - mean
        } else {
            feats
        }
    }

    /// Compute raw (non-CMN) kaldi fbank for a 16 kHz mono waveform — the frontend for
    /// models with NO `feature_normalize_type` metadata (WeSpeaker VoxCeleb exports).
    /// The waveform must already be at the scale the model expects (i16 range for
    /// `normalize_samples=0` models).
    pub fn compute_raw(&self, waveform: &[f32]) -> ndarray::Array2<f32> {
        let n_frames = Self::num_frames(waveform.len());
        if n_frames == 0 {
            return ndarray::Array2::zeros((0, N_MELS));
        }

        let mut feats = ndarray::Array2::<f32>::zeros((n_frames, N_MELS));
        let mut input = self.fft.make_input_vec(); // len N_FFT
        let mut spectrum = self.fft.make_output_vec(); // len N_FREQ
        let mut frame = vec![0f32; FRAME_LENGTH];

        for t in 0..n_frames {
            let start = t * FRAME_SHIFT;
            frame.copy_from_slice(&waveform[start..start + FRAME_LENGTH]);

            // 1) remove DC offset (subtract the frame mean).
            let mean = frame.iter().sum::<f32>() / FRAME_LENGTH as f32;
            for x in frame.iter_mut() {
                *x -= mean;
            }
            // 2) pre-emphasis (kaldi order: high->low, then index 0 last).
            for i in (1..FRAME_LENGTH).rev() {
                frame[i] -= PREEMPH_COEFF * frame[i - 1];
            }
            frame[0] -= PREEMPH_COEFF * frame[0];
            // 3) apply the Povey window and zero-pad to N_FFT.
            for i in 0..FRAME_LENGTH {
                input[i] = frame[i] * self.window[i];
            }
            for x in input.iter_mut().skip(FRAME_LENGTH) {
                *x = 0.0;
            }

            self.fft.process(&mut input, &mut spectrum).expect("rfft");

            // 4) power spectrum -> mel energies -> log(max(e, eps)).
            for m in 0..N_MELS {
                let row = &self.fbank[m * N_FFT_BINS..(m + 1) * N_FFT_BINS];
                let mut acc = 0f32;
                for (k, &w) in row.iter().enumerate() {
                    if w != 0.0 {
                        let c = spectrum[k];
                        acc += w * (c.re * c.re + c.im * c.im);
                    }
                }
                feats[[t, m]] = acc.max(f32::EPSILON).ln();
            }
        }

        feats
    }
}

impl Default for KaldiFbank {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_count_matches_kaldi_snip_edges() {
        // Shorter than one 25 ms frame -> no frames.
        assert_eq!(KaldiFbank::num_frames(0), 0);
        assert_eq!(KaldiFbank::num_frames(399), 0);
        // Exactly one frame.
        assert_eq!(KaldiFbank::num_frames(400), 1);
        // 400 + 160 = 560 -> two frames.
        assert_eq!(KaldiFbank::num_frames(560), 2);
        // 1 second: 1 + (16000-400)/160 = 1 + 97 = 98.
        assert_eq!(KaldiFbank::num_frames(16_000), 98);
        // 250 ms (min turn): 1 + (4000-400)/160 = 1 + 22 = 23.
        assert_eq!(KaldiFbank::num_frames(4_000), 23);
    }

    #[test]
    fn output_shape_and_finiteness() {
        let fb = KaldiFbank::new();
        // 0.5 s sine at 300 Hz.
        let wav: Vec<f32> = (0..8000)
            .map(|i| {
                0.3 * (2.0 * std::f32::consts::PI * 300.0 * i as f32 / SAMPLE_RATE as f32).sin()
            })
            .collect();
        let feats = fb.compute(&wav);
        assert_eq!(feats.shape(), &[KaldiFbank::num_frames(8000), N_MELS]);
        assert!(feats.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn cmn_zeroes_per_bin_mean() {
        let fb = KaldiFbank::new();
        let wav: Vec<f32> = (0..8000)
            .map(|i| {
                0.2 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / SAMPLE_RATE as f32).sin()
            })
            .collect();
        let feats = fb.compute(&wav);
        // After CMN each mel bin's temporal mean is ~0.
        let mean = feats.mean_axis(ndarray::Axis(0)).unwrap();
        for m in mean.iter() {
            assert!(m.abs() < 1e-3, "per-bin mean not ~0 after CMN: {m}");
        }
    }

    #[test]
    fn short_input_yields_empty() {
        let fb = KaldiFbank::new();
        let feats = fb.compute(&[0.0; 100]);
        assert_eq!(feats.shape(), &[0, N_MELS]);
        assert_eq!(fb.compute_raw(&[0.0; 100]).shape(), &[0, N_MELS]);
    }

    #[test]
    fn compute_is_compute_raw_minus_temporal_mean() {
        let fb = KaldiFbank::new();
        let wav: Vec<f32> = (0..8000)
            .map(|i| {
                0.2 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / SAMPLE_RATE as f32).sin()
            })
            .collect();
        let raw = fb.compute_raw(&wav);
        let cmn = fb.compute(&wav);
        let mean = raw.mean_axis(ndarray::Axis(0)).unwrap();
        for t in 0..raw.shape()[0] {
            for m in 0..N_MELS {
                let expect = raw[[t, m]] - mean[m];
                assert!((cmn[[t, m]] - expect).abs() < 1e-5);
            }
        }
        // Raw features are NOT zero-mean (CMN really was skipped).
        let raw_mean = raw.mean_axis(ndarray::Axis(0)).unwrap();
        assert!(raw_mean.iter().any(|m| m.abs() > 0.1));
    }
}
