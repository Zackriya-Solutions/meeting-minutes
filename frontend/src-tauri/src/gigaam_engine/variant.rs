//! GigaAM v3 model variants, selectable for A/B quality testing. All are e2e exports
//! (punctuated, capitalized Russian) from `istupakov/gigaam-v3-onnx`:
//!   - **int8** is ~4× smaller and faster on CPU; **fp32** is the accuracy baseline.
//!   - **RNN-T** generally beats **CTC** on WER, at the cost of a 3-session
//!     autoregressive decode (encoder + prediction network + joiner).
//!   - **Neural Engine** (Apple Silicon only) is RNN-T fp32 with the encoder converted to a
//!     CoreML fp16 MLProgram: same weights, so effectively the same transcripts, but the
//!     encoder leaves the CPU entirely.
//!
//! Every variant's files have distinct names, so all variants coexist in `models/gigaam/`.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GigaamVariant {
    E2eCtcInt8,
    E2eCtcFp32,
    E2eRnntInt8,
    E2eRnntFp32,
    /// e2e-RNN-T with the encoder on the Apple Neural Engine (macOS arm64 only): the ONNX
    /// encoder is replaced by an fp16 CoreML MLProgram, the prediction network and joiner
    /// stay on `ort`. Same weights as [`Self::E2eRnntFp32`], ~10× faster and ~6× lighter on
    /// CPU — see [`crate::gigaam_engine::coreml`].
    E2eRnntAne,
}

pub enum DecodeKind {
    Ctc,
    Rnnt,
}

impl Default for GigaamVariant {
    fn default() -> Self {
        // e2e-RNN-T fp32 — the highest-accuracy variant, matching the GigaType2 dictation
        // app's `gigaam-v3-e2e-rnnt` (fp32, onnx-asr). Existing installs that already have a
        // different variant downloaded keep it (see `commands::read_selected`).
        GigaamVariant::E2eRnntFp32
    }
}

impl GigaamVariant {
    pub const ALL: [GigaamVariant; 5] = [
        GigaamVariant::E2eCtcInt8,
        GigaamVariant::E2eCtcFp32,
        GigaamVariant::E2eRnntInt8,
        GigaamVariant::E2eRnntFp32,
        GigaamVariant::E2eRnntAne,
    ];

    /// Stable id persisted to disk and exchanged with the frontend.
    pub fn id(self) -> &'static str {
        match self {
            GigaamVariant::E2eCtcInt8 => "e2e-ctc-int8",
            GigaamVariant::E2eCtcFp32 => "e2e-ctc-fp32",
            GigaamVariant::E2eRnntInt8 => "e2e-rnnt-int8",
            GigaamVariant::E2eRnntFp32 => "e2e-rnnt-fp32",
            GigaamVariant::E2eRnntAne => "e2e-rnnt-ane",
        }
    }

    pub fn from_id(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.id() == s)
    }

    /// Human-readable label for the settings dropdown.
    pub fn label(self) -> &'static str {
        match self {
            GigaamVariant::E2eCtcInt8 => "e2e-CTC · int8 (default — fastest, smallest)",
            GigaamVariant::E2eCtcFp32 => "e2e-CTC · fp32 (accuracy baseline)",
            GigaamVariant::E2eRnntInt8 => "e2e-RNN-T · int8 (better WER, small)",
            GigaamVariant::E2eRnntFp32 => "e2e-RNN-T · fp32 (best WER)",
            GigaamVariant::E2eRnntAne => "e2e-RNN-T · Neural Engine (fastest on Apple Silicon)",
        }
    }

    /// Approximate total download size in MB (sum of all files for the variant).
    pub fn approx_mb(self) -> u32 {
        match self {
            GigaamVariant::E2eCtcInt8 => 225,
            GigaamVariant::E2eCtcFp32 => 886,
            GigaamVariant::E2eRnntInt8 => 227,
            GigaamVariant::E2eRnntFp32 => 891,
            // 409 MB CoreML archive + the 6 MB decoder/joiner/vocab — no ONNX encoder.
            GigaamVariant::E2eRnntAne => 415,
        }
    }

    pub fn decode_kind(self) -> DecodeKind {
        match self {
            GigaamVariant::E2eCtcInt8 | GigaamVariant::E2eCtcFp32 => DecodeKind::Ctc,
            GigaamVariant::E2eRnntInt8 | GigaamVariant::E2eRnntFp32 | GigaamVariant::E2eRnntAne => {
                DecodeKind::Rnnt
            }
        }
    }

    /// True when the encoder is the CoreML/Neural Engine model rather than ONNX. Such
    /// variants need [`Self::ane_asset`] downloaded and compiled next to the ONNX files.
    pub fn uses_ane_encoder(self) -> bool {
        matches!(self, GigaamVariant::E2eRnntAne)
    }

    /// The CoreML encoder archive for this variant, as published by
    /// [gigaam-v3-coreml](https://github.com/IsaacClarke2/gigaam-v3-coreml).
    ///
    /// fp16 rather than the int8 asset (`gigaam-v3-encoder-ane-int8.mlpackage.zip`, ~195 MB):
    /// int8 measures the same on accuracy, but fp16 is what the reference app ships and what
    /// the timings above were taken on. Switching is a one-line change here.
    pub fn ane_asset(self) -> Option<&'static str> {
        match self {
            GigaamVariant::E2eRnntAne => Some("gigaam-v3-encoder-ane.mlpackage.zip"),
            _ => None,
        }
    }

    /// Vocab file for this variant (CTC and RNN-T ship distinct vocabularies).
    pub fn vocab_file(self) -> &'static str {
        match self.decode_kind() {
            DecodeKind::Ctc => "v3_e2e_ctc_vocab.txt",
            DecodeKind::Rnnt => "v3_e2e_rnnt_vocab.txt",
        }
    }

    /// ONNX model file(s), excluding the vocab. CTC has one; RNN-T has encoder,
    /// decoder, joiner (in that order — `RnntModel::load` relies on it). The Neural Engine
    /// variant has no ONNX encoder at all: decoder, joiner (that order —
    /// `load_global` relies on it).
    pub fn model_files(self) -> Vec<&'static str> {
        match self {
            GigaamVariant::E2eCtcInt8 => vec!["v3_e2e_ctc.int8.onnx"],
            GigaamVariant::E2eCtcFp32 => vec!["v3_e2e_ctc.onnx"],
            GigaamVariant::E2eRnntInt8 => vec![
                "v3_e2e_rnnt_encoder.int8.onnx",
                "v3_e2e_rnnt_decoder.int8.onnx",
                "v3_e2e_rnnt_joint.int8.onnx",
            ],
            GigaamVariant::E2eRnntFp32 => vec![
                "v3_e2e_rnnt_encoder.onnx",
                "v3_e2e_rnnt_decoder.onnx",
                "v3_e2e_rnnt_joint.onnx",
            ],
            GigaamVariant::E2eRnntAne => {
                vec!["v3_e2e_rnnt_decoder.onnx", "v3_e2e_rnnt_joint.onnx"]
            }
        }
    }

    /// All Hugging Face files that must be present locally / downloaded (vocab + ONNX). The
    /// CoreML encoder of an ANE variant comes from elsewhere and is tracked separately —
    /// see [`Self::ane_asset`] and `coreml::is_compiled_model_usable`.
    pub fn all_files(self) -> Vec<&'static str> {
        let mut files = vec![self.vocab_file()];
        files.extend(self.model_files());
        files
    }
}
