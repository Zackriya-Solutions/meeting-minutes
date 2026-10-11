//! CAM++ speaker embeddings from Kaldi filterbank features.
use super::fbank::{Fbank, NUM_MEL_BINS};
use super::{model_metadata, session_builder};
use anyhow::{anyhow, Context, Result};
use ndarray::Array3;
use ort::inputs;
use ort::session::Session;
use ort::value::TensorRef;
use sha2::{Digest, Sha256};
use std::path::Path;

const INPUT: &str = "x";
const OUTPUT: &str = "embedding";

/// SHA-256 of the pinned CAM++ model after `fix_average_pool_divisor`.
pub(crate) const PATCHED_SHA256: &str = "a9dda3bb22ddb88504012891d87544f8d68fc859f559f4520eab10cf51c4c7a3";

/// A serialized ONNX AttributeProto `count_include_pad = 1`: field 1 (name, 17 bytes) then field 3 (int, varint 1).
const COUNT_INCLUDE_PAD_ONE: &[u8] = b"\x0a\x11count_include_pad\x18\x01";

/// Copy of `model` with every `count_include_pad = 1` rewritten to `0`; the length is unchanged.
///
/// CAM++ pools segments with AveragePool(ceil_mode=1, count_include_pad=1, pads=0). onnxruntime
/// before 1.29 divides the overhanging last window by the full kernel instead of by the frames it
/// covers, which shrinks that segment's statistics and corrupts most embeddings (fixed in
/// onnxruntime 1.29). With zero pads the only "padding" is that
/// overhang, so excluding it gives exactly the PyTorch result on every onnxruntime version, and
/// the patch is a no-op where the bug is fixed. Returns the patched bytes and the number of
/// attributes rewritten.
pub(crate) fn fix_average_pool_divisor(model: &[u8]) -> (Vec<u8>, usize) {
    let mut out = model.to_vec();
    let n = COUNT_INCLUDE_PAD_ONE.len();
    let mut count = 0;
    let mut i = 0;
    while i + n <= out.len() {
        if out[i] == COUNT_INCLUDE_PAD_ONE[0] && &out[i..i + n] == COUNT_INCLUDE_PAD_ONE {
            out[i + n - 1] = 0;
            count += 1;
            i += n;
        } else {
            i += 1;
        }
    }
    (out, count)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub struct EmbeddingModel {
    session: Session,
    fbank: Fbank,
    /// Model expects int16-scaled samples (metadata normalize_samples = 0).
    scale_to_int16: bool,
    /// Subtract the per-utterance mean of each feature dimension.
    global_mean: bool,
    /// Embedding size from the model metadata (512 for CAM++ VoxCeleb), when present.
    output_dim: Option<usize>,
}

impl EmbeddingModel {
    pub fn load(path: &Path) -> Result<Self> {
        crate::ensure_onnx_runtime_available()?;
        // The downloaded file stays byte-for-byte as pinned; the pooling fix is applied in memory.
        let original =
            std::fs::read(path).with_context(|| format!("Failed to read speaker model {}", path.display()))?;
        let (model, patched) = fix_average_pool_divisor(&original);
        if sha256_hex(&model) != PATCHED_SHA256 {
            return Err(anyhow!(
                "The speaker model file {} is damaged. Delete the speaker models and download them again.",
                path.display()
            ));
        }
        log::debug!("Corrected {} speaker-model pooling layers for onnxruntime before 1.29", patched);
        let session = session_builder()?.commit_from_memory(&model)?;
        let meta = |key: &str| model_metadata(&session, key);
        let scale_to_int16 = meta("normalize_samples").map(|v| v == "0").unwrap_or(false);
        let global_mean = meta("feature_normalize_type").map(|v| v == "global-mean").unwrap_or(true);
        let sample_rate: usize = meta("sample_rate").and_then(|v| v.parse().ok()).unwrap_or(16_000);
        let output_dim: Option<usize> = meta("output_dim").and_then(|v| v.parse().ok());
        log::info!(
            "Loaded speaker embedding model: int16_scale={} global_mean={} output_dim={:?}",
            scale_to_int16, global_mean, output_dim
        );
        Ok(Self { session, fbank: Fbank::new(sample_rate), scale_to_int16, global_mean, output_dim })
    }

    pub fn embed(&mut self, samples: &[f32]) -> Result<Vec<f32>> {
        let scaled: Vec<f32>;
        let input_samples = if self.scale_to_int16 {
            scaled = samples.iter().map(|x| x * 32768.0).collect();
            &scaled[..]
        } else {
            samples
        };
        let mut feats = self.fbank.compute(input_samples);
        if feats.is_empty() {
            return Err(anyhow!("audio too short for a speaker embedding"));
        }
        if self.global_mean {
            let n = feats.len() as f32;
            let mut mean = [0f32; NUM_MEL_BINS];
            for f in &feats {
                for (m, x) in mean.iter_mut().zip(f) {
                    *m += x / n;
                }
            }
            for f in &mut feats {
                for (x, m) in f.iter_mut().zip(&mean) {
                    *x -= m;
                }
            }
        }
        let mut input = Array3::<f32>::zeros((1, feats.len(), NUM_MEL_BINS));
        for (t, f) in feats.iter().enumerate() {
            for (d, &x) in f.iter().enumerate() {
                input[[0, t, d]] = x;
            }
        }
        let outputs = self.session.run(inputs![INPUT => TensorRef::from_array_view(input.view())?])?;
        let emb: Vec<f32> = outputs
            .get(OUTPUT)
            .ok_or_else(|| anyhow!("embedding output '{OUTPUT}' missing"))?
            .try_extract_array::<f32>()?
            .iter()
            .copied()
            .collect();
        if let Some(d) = self.output_dim {
            if emb.len() != d {
                return Err(anyhow!("embedding length {} != model output_dim {}", emb.len(), d));
            }
        }
        Ok(emb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAME: &[u8] = b"\x0a\x11count_include_pad";

    #[test]
    fn average_pool_divisor_patch_flips_only_count_include_pad_one() {
        let mut model = Vec::new();
        let mut expected = Vec::new();
        let mut both = |original: &[u8], patched: &[u8]| {
            model.extend_from_slice(original);
            expected.extend_from_slice(patched);
        };
        both(b"head", b"head");
        both(NAME, NAME);
        both(b"\x18\x01", b"\x18\x00");
        both(b"\xa0\x01\x02 other \x18\x01", b"\xa0\x01\x02 other \x18\x01");
        both(NAME, NAME);
        both(b"\x18\x00", b"\x18\x00");
        both(NAME, NAME);
        both(b"\x18\x01tail", b"\x18\x00tail");

        let (patched, count) = fix_average_pool_divisor(&model);
        assert_eq!(count, 2);
        assert_eq!(patched.len(), model.len());
        assert_eq!(patched, expected);
    }

    #[test]
    fn average_pool_divisor_patch_leaves_other_models_alone() {
        let model = b"no pooling attributes here \x18\x01".to_vec();
        assert_eq!(fix_average_pool_divisor(&model), (model.clone(), 0));
    }

    #[test]
    #[ignore = "needs DIARIZATION_REF_DIR with the speaker embedding model"]
    fn reference_model_patches_every_average_pool() {
        let dir = std::path::PathBuf::from(std::env::var("DIARIZATION_REF_DIR").expect("DIARIZATION_REF_DIR"));
        let original = std::fs::read(dir.join(crate::diarization::models::EMBEDDING.file_name)).unwrap();
        let (patched, count) = fix_average_pool_divisor(&original);
        assert_eq!(count, 52);
        assert_eq!(patched.len(), original.len());
        assert_eq!(sha256_hex(&patched), PATCHED_SHA256);
    }
}
