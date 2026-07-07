//! GigaAM v3 log-mel featurizer — an exact Rust port of onnx-asr's
//! `GigaamPreprocessorNumpy` (v3). The `istupakov/gigaam-v3-onnx` models expect
//! `features` of shape [1, 64, T]; the featurizer is NOT baked into the ONNX, so we
//! reproduce it here bit-for-bit.
//!
//! v3 params (from onnx-asr + the model yaml): 16 kHz, `n_fft = win_length = 320`,
//! `hop = 160`, **no centering/padding** (`center=false`), a precomputed analysis
//! window and mel filterbank (bundled from onnx-asr's `fbanks.npz`), power spectrum,
//! `log(clip(mel, 1e-9, 1e9))`, no CMVN.

use std::sync::Arc;

use realfft::{RealFftPlanner, RealToComplex};

pub const SAMPLE_RATE: usize = 16_000;
pub const N_FFT: usize = 320;
pub const WIN_LEN: usize = 320;
pub const HOP: usize = 160;
pub const N_MELS: usize = 64;
pub const N_FREQ: usize = N_FFT / 2 + 1; // 161
const CLAMP_MIN: f32 = 1e-9;
const CLAMP_MAX: f32 = 1e9;

// Bundled precomputed arrays (little-endian f32, row-major), from onnx-asr fbanks.npz.
static WINDOW_BYTES: &[u8] = include_bytes!("../../resources/gigaam/window_v3.f32"); // [320]
static FBANK_BYTES: &[u8] = include_bytes!("../../resources/gigaam/mel_fbank_v3.f32"); // [161 x 64]

// Parse little-endian f32 without an alignment requirement. `include_bytes!` yields a
// byte slice with alignment 1, so `bytemuck::cast_slice::<u8, f32>` panics
// (TargetAlignmentGreaterAndInputNotAligned) whenever the embedded blob isn't 4-byte
// aligned. `from_le_bytes` copies element-by-element and is alignment-agnostic. The
// assets are stored little-endian (numpy `tofile` on this LE host).
fn as_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

pub struct Featurizer {
    window: Vec<f32>,           // [WIN_LEN]
    fbank: Vec<f32>,            // [N_FREQ * N_MELS], row-major: fbank[freq*N_MELS + mel]
    fft: Arc<dyn RealToComplex<f32>>,
}

impl Featurizer {
    pub fn new() -> Self {
        let window = as_f32(WINDOW_BYTES);
        let fbank = as_f32(FBANK_BYTES);
        debug_assert_eq!(window.len(), WIN_LEN);
        debug_assert_eq!(fbank.len(), N_FREQ * N_MELS);
        let mut planner = RealFftPlanner::<f32>::new();
        Self { window, fbank, fft: planner.plan_fft_forward(N_FFT) }
    }

    /// Number of frames for a waveform of `n_samples` (no padding).
    pub fn num_frames(n_samples: usize) -> usize {
        if n_samples < WIN_LEN {
            0
        } else {
            (n_samples - WIN_LEN) / HOP + 1
        }
    }

    /// Compute log-mel features for a 16 kHz mono waveform.
    /// Returns `(feats, n_frames)` where `feats` is row-major `[N_MELS, n_frames]`
    /// (`feats[mel * n_frames + t]`) — the layout the ONNX `features` tensor expects
    /// once a batch dim is prepended.
    pub fn compute(&self, waveform: &[f32]) -> (Vec<f32>, usize) {
        let n_frames = Self::num_frames(waveform.len());
        if n_frames == 0 {
            return (Vec::new(), 0);
        }

        let mut feats = vec![0f32; N_MELS * n_frames];
        let mut input = self.fft.make_input_vec(); // len N_FFT
        let mut spectrum = self.fft.make_output_vec(); // len N_FREQ
        let mut power = vec![0f32; N_FREQ];

        for t in 0..n_frames {
            let start = t * HOP;
            // Windowed frame (N_FFT == WIN_LEN, so the input is fully filled).
            for i in 0..WIN_LEN {
                input[i] = waveform[start + i] * self.window[i];
            }
            self.fft.process(&mut input, &mut spectrum).expect("rfft");

            for (f, c) in spectrum.iter().enumerate() {
                power[f] = c.re * c.re + c.im * c.im;
            }
            // mel[m] = Σ_f power[f] * fbank[f, m]; then log(clip).
            for m in 0..N_MELS {
                let mut acc = 0f32;
                for f in 0..N_FREQ {
                    acc += power[f] * self.fbank[f * N_MELS + m];
                }
                feats[m * n_frames + t] = acc.clamp(CLAMP_MIN, CLAMP_MAX).ln();
            }
        }
        (feats, n_frames)
    }
}

impl Default for Featurizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Deterministic 1s signal identical to the numpy reference generator.
    fn ref_waveform() -> Vec<f32> {
        (0..SAMPLE_RATE)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
                    + 0.3 * (2.0 * std::f32::consts::PI * 880.0 * t).sin()
            })
            .collect()
    }

    #[test]
    fn matches_onnx_asr_reference() {
        let f = Featurizer::new();
        let wav = ref_waveform();
        let (feats, t) = f.compute(&wav);
        assert_eq!(t, 99, "frame count");
        assert_eq!(feats.len(), N_MELS * 99);

        // Reference values from onnx-asr's GigaamPreprocessorNumpy (see extraction step).
        // feats[m, 0] for m=0..3:
        let col0: Vec<f32> = (0..4).map(|m| feats[m * t + 0]).collect();
        let expect0 = [-9.219525, -7.973648, -7.978273, -8.218164];
        for (g, e) in col0.iter().zip(expect0) {
            assert!((g - e).abs() < 2e-3, "col0 got {g} expected {e}");
        }
        // feats[m, 10] for m=0..3:
        let col10: Vec<f32> = (0..4).map(|m| feats[m * t + 10]).collect();
        let expect10 = [-9.216023, -7.970147, -7.979, -8.218892];
        for (g, e) in col10.iter().zip(expect10) {
            assert!((g - e).abs() < 2e-3, "col10 got {g} expected {e}");
        }
        // Aggregate sanity.
        let sum: f32 = feats.iter().sum();
        assert!((sum - (-38594.586)).abs() < 5.0, "sum drift: {sum}");
    }
}
