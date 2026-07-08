//! GigaAM v3 model variants, selectable for A/B quality testing. All are e2e exports
//! (punctuated, capitalized Russian) from `istupakov/gigaam-v3-onnx`:
//!   - **int8** is ~4× smaller and faster on CPU; **fp32** is the accuracy baseline.
//!   - **RNN-T** generally beats **CTC** on WER, at the cost of a 3-session
//!     autoregressive decode (encoder + prediction network + joiner).
//!
//! Every variant's files have distinct names, so all variants coexist in `models/gigaam/`.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GigaamVariant {
    E2eCtcInt8,
    E2eCtcFp32,
    E2eRnntInt8,
    E2eRnntFp32,
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
    pub const ALL: [GigaamVariant; 4] = [
        GigaamVariant::E2eCtcInt8,
        GigaamVariant::E2eCtcFp32,
        GigaamVariant::E2eRnntInt8,
        GigaamVariant::E2eRnntFp32,
    ];

    /// Stable id persisted to disk and exchanged with the frontend.
    pub fn id(self) -> &'static str {
        match self {
            GigaamVariant::E2eCtcInt8 => "e2e-ctc-int8",
            GigaamVariant::E2eCtcFp32 => "e2e-ctc-fp32",
            GigaamVariant::E2eRnntInt8 => "e2e-rnnt-int8",
            GigaamVariant::E2eRnntFp32 => "e2e-rnnt-fp32",
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
        }
    }

    /// Approximate total download size in MB (sum of all files for the variant).
    pub fn approx_mb(self) -> u32 {
        match self {
            GigaamVariant::E2eCtcInt8 => 225,
            GigaamVariant::E2eCtcFp32 => 886,
            GigaamVariant::E2eRnntInt8 => 227,
            GigaamVariant::E2eRnntFp32 => 891,
        }
    }

    pub fn decode_kind(self) -> DecodeKind {
        match self {
            GigaamVariant::E2eCtcInt8 | GigaamVariant::E2eCtcFp32 => DecodeKind::Ctc,
            GigaamVariant::E2eRnntInt8 | GigaamVariant::E2eRnntFp32 => DecodeKind::Rnnt,
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
    /// decoder, joiner (in that order — `RnntModel::load` relies on it).
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
        }
    }

    /// All files that must be present locally / downloaded (vocab + model).
    pub fn all_files(self) -> Vec<&'static str> {
        let mut files = vec![self.vocab_file()];
        files.extend(self.model_files());
        files
    }
}
