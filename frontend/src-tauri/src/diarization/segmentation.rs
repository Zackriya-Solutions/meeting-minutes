//! pyannote segmentation-3.0: per-frame activity of up to 3 local speakers per 10 s window.
use super::reconstruct::FrameGeometry;
use super::{model_metadata, session_builder};
use anyhow::{anyhow, Result};
use ndarray::{Array3, Ix3};
use ort::inputs;
use ort::session::Session;
use ort::value::TensorRef;
use std::path::Path;

const INPUT: &str = "x";
const OUTPUT: &str = "y";

pub struct SegmentationModel {
    session: Session,
    pub window_samples: usize,
    pub geometry: FrameGeometry,
}

fn meta_usize(session: &Session, key: &str, default: usize) -> usize {
    model_metadata(session, key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

impl SegmentationModel {
    pub fn load(path: &Path) -> Result<Self> {
        crate::ensure_onnx_runtime_available()?;
        let session = session_builder()?.commit_from_file(path)?;
        let window_samples = meta_usize(&session, "window_size", 160_000);
        let geometry = FrameGeometry {
            frame_shift: meta_usize(&session, "receptive_field_shift", 270),
            frame_size: meta_usize(&session, "receptive_field_size", 991),
            sample_rate: meta_usize(&session, "sample_rate", 16_000),
        };
        log::info!("Loaded segmentation model: window={} geometry={:?}", window_samples, geometry);
        Ok(Self { session, window_samples, geometry })
    }

    /// Powerset scores (frames × 7) for each window. Windows shorter than `window_samples` are
    /// zero-padded; longer ones are cut.
    pub fn run_batch(&mut self, windows: &[&[f32]]) -> Result<Vec<Vec<[f32; 7]>>> {
        let len = self.window_samples;
        let mut input = Array3::<f32>::zeros((windows.len(), 1, len));
        let rows = input.as_slice_mut().expect("a new array is contiguous").chunks_exact_mut(len);
        for (row, w) in rows.zip(windows) {
            let n = w.len().min(len);
            row[..n].copy_from_slice(&w[..n]);
        }
        let outputs = self.session.run(inputs![INPUT => TensorRef::from_array_view(input.view())?])?;
        let scores = outputs
            .get(OUTPUT)
            .ok_or_else(|| anyhow!("segmentation output '{OUTPUT}' missing"))?
            .try_extract_array::<f32>()?
            .into_dimensionality::<Ix3>()?;
        let (batch, frames, classes) = scores.dim();
        if classes != 7 {
            return Err(anyhow!("unexpected segmentation output classes {classes}"));
        }
        Ok((0..batch)
            .map(|b| {
                (0..frames)
                    .map(|f| {
                        let mut row = [0f32; 7];
                        for (c, slot) in row.iter_mut().enumerate() {
                            *slot = scores[[b, f, c]];
                        }
                        row
                    })
                    .collect()
            })
            .collect())
    }
}
