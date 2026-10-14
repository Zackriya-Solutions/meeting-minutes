//! Neural echo cancellation core: DTLN-aec (two small ONNX models with LSTM
//! state), streamed in 8 ms hops at 16 kHz. Ported from Hyprnote/anarlog
//! (crates/aec, MIT); the models are DTLN-aec by Westhausen & Meyer (MIT).
//!
//! The model has no delay search: the caller must feed the reference slightly
//! ahead of its echo (see `echo_cancellation`). Output trails input by
//! `LATENCY` samples (overlap-add of 512-sample blocks).

use ndarray::{Array3, Array4};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::TensorRef;
use realfft::{num_complex::Complex, ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

/// DTLN-aec "128" models (MIT, see resources/dtln-aec/LICENSE).
pub const MODEL_1: &[u8] = include_bytes!("../../resources/dtln-aec/model_128_1.onnx");
pub const MODEL_2: &[u8] = include_bytes!("../../resources/dtln-aec/model_128_2.onnx");
pub const MODEL_STATE_SIZE: usize = 128;
const BLOCK: usize = 512;
const SHIFT: usize = 128;
/// Output delay relative to input, in samples.
pub const LATENCY: usize = BLOCK - SHIFT;
/// Samples consumed per model step (8 ms).
pub const HOP: usize = SHIFT;

pub struct NeuralEcho {
    model_1: Session,
    model_2: Session,
    states_1: Array4<f32>,
    states_2: Array4<f32>,
    fft: Arc<dyn RealToComplex<f32>>,
    ifft: Arc<dyn ComplexToReal<f32>>,
    mic_block: Vec<f32>,
    ref_block: Vec<f32>,
    out_block: Vec<f32>,
    pending_mic: Vec<f32>,
    pending_ref: Vec<f32>,
    failed_hops: u64,
    // scratch
    spec: Vec<Complex<f32>>,
    ref_spec: Vec<Complex<f32>>,
    fft_in: Vec<f32>,
    scratch: Vec<Complex<f32>>,
}

fn session(model: &[u8]) -> ort::Result<Session> {
    // Small per-frame models: one thread is fastest and avoids contention.
    Session::builder()?
        .with_intra_threads(1)?
        .with_inter_threads(1)?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .commit_from_memory(model)
}

impl NeuralEcho {
    pub fn new(model_1: &[u8], model_2: &[u8], state_size: usize) -> ort::Result<Self> {
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(BLOCK);
        let ifft = planner.plan_fft_inverse(BLOCK);
        let scratch_len = fft.get_scratch_len().max(ifft.get_scratch_len());
        Ok(Self {
            model_1: session(model_1)?,
            model_2: session(model_2)?,
            states_1: Array4::zeros((1, 2, state_size, 2)),
            states_2: Array4::zeros((1, 2, state_size, 2)),
            fft,
            ifft,
            mic_block: vec![0.0; BLOCK],
            ref_block: vec![0.0; BLOCK],
            out_block: vec![0.0; BLOCK],
            pending_mic: Vec::new(),
            pending_ref: Vec::new(),
            failed_hops: 0,
            spec: vec![Complex::default(); BLOCK / 2 + 1],
            ref_spec: vec![Complex::default(); BLOCK / 2 + 1],
            fft_in: vec![0.0; BLOCK],
            scratch: vec![Complex::default(); scratch_len],
        })
    }

    /// Feed aligned 16 kHz mic and reference of equal length; returns one
    /// output sample per input sample once a full hop is available (the
    /// remainder waits for the next call). A hop the model fails on passes the
    /// mic through, on the same timeline.
    pub fn process(&mut self, mic: &[f32], reference: &[f32]) -> Vec<f32> {
        let n = mic.len().min(reference.len());
        self.pending_mic.extend_from_slice(&mic[..n]);
        self.pending_ref.extend_from_slice(&reference[..n]);
        let hops = self.pending_mic.len() / SHIFT;
        let mut out = Vec::with_capacity(hops * SHIFT);
        for h in 0..hops {
            let r = h * SHIFT..(h + 1) * SHIFT;
            self.mic_block.copy_within(SHIFT.., 0);
            self.mic_block[BLOCK - SHIFT..].copy_from_slice(&self.pending_mic[r.clone()]);
            self.ref_block.copy_within(SHIFT.., 0);
            self.ref_block[BLOCK - SHIFT..].copy_from_slice(&self.pending_ref[r]);
            match self.hop() {
                Ok(()) => out.extend_from_slice(&self.out_block[..SHIFT]),
                Err(e) => {
                    self.failed_hops += 1;
                    if self.failed_hops == 1 || self.failed_hops % 1000 == 0 {
                        log::warn!("Neural echo cancellation failed for {} hops: {e}", self.failed_hops);
                    }
                    // The mic `LATENCY` samples ago lines up with the output.
                    out.extend_from_slice(&self.mic_block[..SHIFT]);
                    self.out_block.copy_within(SHIFT.., 0);
                    self.out_block[BLOCK - SHIFT..].fill(0.0);
                }
            }
        }
        self.pending_mic.drain(..hops * SHIFT);
        self.pending_ref.drain(..hops * SHIFT);
        out
    }

    fn hop(&mut self) -> ort::Result<()> {
        let bins = BLOCK / 2 + 1;
        // Magnitude spectra of the current mic and reference blocks.
        self.fft_in.copy_from_slice(&self.mic_block);
        let _ = self.fft.process_with_scratch(&mut self.fft_in, &mut self.spec, &mut self.scratch);
        self.fft_in.copy_from_slice(&self.ref_block);
        let _ = self.fft.process_with_scratch(&mut self.fft_in, &mut self.ref_spec, &mut self.scratch);
        let mic_mag = Array3::from_shape_fn((1, 1, bins), |(_, _, i)| self.spec[i].norm());
        let ref_mag = Array3::from_shape_fn((1, 1, bins), |(_, _, i)| self.ref_spec[i].norm());

        // Stage 1: spectral mask.
        let outputs = self.model_1.run(ort::inputs![
            TensorRef::from_array_view(mic_mag.view())?,
            TensorRef::from_array_view(self.states_1.view())?,
            TensorRef::from_array_view(ref_mag.view())?
        ])?;
        let mask = outputs["Identity"].try_extract_array::<f32>()?.iter().copied().collect::<Vec<_>>();
        let states = outputs["Identity_1"].try_extract_array::<f32>()?;
        self.states_1.assign(&states.view().into_shape_with_order(self.states_1.raw_dim()).map_err(|e| ort::Error::new(e.to_string()))?);
        drop(outputs);
        for (c, m) in self.spec.iter_mut().zip(&mask) {
            *c *= *m;
        }
        let mut estimate = vec![0.0f32; BLOCK];
        let _ = self.ifft.process_with_scratch(&mut self.spec, &mut estimate, &mut self.scratch);
        let estimate = Array3::from_shape_fn((1, 1, BLOCK), |(_, _, i)| estimate[i] / BLOCK as f32);
        let ref_time = Array3::from_shape_fn((1, 1, BLOCK), |(_, _, i)| self.ref_block[i]);

        // Stage 2: time-domain refinement.
        let outputs = self.model_2.run(ort::inputs![
            TensorRef::from_array_view(estimate.view())?,
            TensorRef::from_array_view(self.states_2.view())?,
            TensorRef::from_array_view(ref_time.view())?
        ])?;
        let block = outputs["Identity"].try_extract_array::<f32>()?.iter().copied().collect::<Vec<_>>();
        let states = outputs["Identity_1"].try_extract_array::<f32>()?;
        self.states_2.assign(&states.view().into_shape_with_order(self.states_2.raw_dim()).map_err(|e| ort::Error::new(e.to_string()))?);

        // Overlap-add.
        self.out_block.copy_within(SHIFT.., 0);
        self.out_block[BLOCK - SHIFT..].fill(0.0);
        for (o, b) in self.out_block.iter_mut().zip(&block) {
            *o += *b;
        }
        Ok(())
    }
}
