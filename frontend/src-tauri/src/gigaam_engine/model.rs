//! GigaAM v3 e2e CTC model: ONNX inference + greedy CTC decode.
//!
//! Interface (from `istupakov/gigaam-v3-onnx`, verified): `v3_e2e_ctc(.int8).onnx`
//!   IN  `features` f32[1,64,T], `feature_lengths` i64[1]
//!   OUT `log_probs` f32[1,T',257]   (257 = 256 BPE tokens + CTC blank at index 256)
//! Decode: per-frame argmax, collapse consecutive repeats, drop blank → token ids →
//! `v3_e2e_ctc_vocab.txt` → replace `▁` with space. Output is punctuated + capitalized.

use std::path::Path;

use anyhow::{anyhow, Result};
use ndarray::{Array1, Array3};

use super::featurizer::{Featurizer, N_MELS};

/// CTC blank index for the e2e-ctc vocab (last class; vocab has 257 entries).
pub const CTC_BLANK: usize = 256;

pub struct GigaamModel {
    session: ort::session::Session,
    featurizer: Featurizer,
    /// index -> token string (id 256 = "<blk>", skipped in decode).
    vocab: Vec<String>,
}

impl GigaamModel {
    pub fn load(model_path: &Path, vocab_path: &Path) -> Result<Self> {
        let vocab = load_vocab(vocab_path)?;
        let session = build_session(model_path)?;
        Ok(Self { session, featurizer: Featurizer::new(), vocab })
    }

    /// Transcribe a 16 kHz mono waveform to punctuated Russian text.
    pub fn transcribe(&mut self, waveform: &[f32]) -> Result<String> {
        use ort::inputs;
        use ort::value::TensorRef;

        let (feats, t) = self.featurizer.compute(waveform);
        if t == 0 {
            return Ok(String::new());
        }
        // feats is row-major [N_MELS, t] → [1, N_MELS, t].
        let features = Array3::from_shape_vec((1, N_MELS, t), feats)
            .map_err(|e| anyhow!("feature reshape: {e}"))?;
        let lengths = Array1::from_vec(vec![t as i64]);

        // Inference + greedy CTC in a scope so the `&mut self.session` borrow (held by
        // `outputs`) is released before we call `self.decode`.
        let ids: Vec<usize> = {
            let feat_ref = TensorRef::from_array_view(features.view())
                .map_err(|e| anyhow!("ort features: {e}"))?;
            let len_ref = TensorRef::from_array_view(lengths.view())
                .map_err(|e| anyhow!("ort feature_lengths: {e}"))?;
            let outputs = self
                .session
                .run(inputs!["features" => feat_ref, "feature_lengths" => len_ref])
                .map_err(|e| anyhow!("ort run: {e}"))?;

            let value = outputs
                .get("log_probs")
                .ok_or_else(|| anyhow!("model output 'log_probs' missing"))?;
            let log_probs = value.try_extract_array::<f32>().map_err(|e| anyhow!("ort extract: {e}"))?;
            let shape = log_probs.shape(); // [1, T', 257]
            let frames = shape[1];
            let classes = shape[2];

            // Greedy CTC: argmax per frame, collapse repeats, drop blank. Setting `prev`
            // even on blank lets a token repeat across a blank (standard CTC collapse).
            let mut ids: Vec<usize> = Vec::new();
            let mut prev = usize::MAX;
            for ti in 0..frames {
                let mut best = 0usize;
                let mut best_v = f32::NEG_INFINITY;
                for c in 0..classes {
                    let v = log_probs[[0, ti, c]];
                    if v > best_v {
                        best_v = v;
                        best = c;
                    }
                }
                if best != CTC_BLANK && best != prev {
                    ids.push(best);
                }
                prev = best;
            }
            ids
        };
        Ok(self.decode(&ids))
    }

    fn decode(&self, ids: &[usize]) -> String {
        let mut s = String::new();
        for &id in ids {
            if let Some(tok) = self.vocab.get(id) {
                s.push_str(tok);
            }
        }
        // SentencePiece word boundary marker → space.
        s.replace('\u{2581}', " ").trim().to_string()
    }
}

/// Parse a `token id` per-line vocab (id is the last whitespace-separated field, so BPE
/// tokens containing no spaces parse cleanly, e.g. `▁с 21`, `. 2`, `<blk> 256`).
fn load_vocab(path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow!("read vocab {}: {e}", path.display()))?;
    let mut entries: Vec<(usize, String)> = Vec::new();
    let mut max_id = 0usize;
    for line in content.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        let (tok, id) = line
            .rsplit_once(char::is_whitespace)
            .ok_or_else(|| anyhow!("bad vocab line: {line:?}"))?;
        let id: usize = id.trim().parse().map_err(|_| anyhow!("bad vocab id: {line:?}"))?;
        max_id = max_id.max(id);
        entries.push((id, tok.to_string()));
    }
    let mut vocab = vec![String::new(); max_id + 1];
    for (id, tok) in entries {
        vocab[id] = tok;
    }
    Ok(vocab)
}

fn build_session(model_path: &Path) -> Result<ort::session::Session> {
    use ort::session::{builder::GraphOptimizationLevel, Session};
    Session::builder()
        .map_err(|e| anyhow!("ort builder: {e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| anyhow!("ort opt level: {e}"))?
        .commit_from_file(model_path)
        .map_err(|e| anyhow!("ort load {}: {e}", model_path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn vocab_parses_bpe_and_blank() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "<unk> 0\n▁ 1\n. 2\n▁с 21\n<blk> 256").unwrap();
        let v = load_vocab(f.path()).unwrap();
        assert_eq!(v.len(), 257);
        assert_eq!(v[0], "<unk>");
        assert_eq!(v[1], "▁");
        assert_eq!(v[2], ".");
        assert_eq!(v[21], "▁с");
        assert_eq!(v[256], "<blk>");
    }
}
