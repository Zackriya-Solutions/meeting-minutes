//! GigaAM v3 model variants, selectable for A/B quality testing. All are e2e exports
//! (punctuated, capitalized) — the Russian-only ones from `istupakov/gigaam-v3-onnx`, the
//! bilingual one from the Yandex Disk archive in [`GigaamVariant::archive_url`]:
//!   - **int8** is ~4× smaller and faster on CPU; **fp32** is the accuracy baseline.
//!   - **RNN-T** generally beats **CTC** on WER, at the cost of a 3-session
//!     autoregressive decode (encoder + prediction network + joiner).
//!   - **Neural Engine** (Apple Silicon only) is RNN-T fp32 with the encoder converted to a
//!     CoreML fp16 MLProgram: same weights, so effectively the same transcripts, but the
//!     encoder leaves the CPU entirely.
//!   - **RU+EN** is the same RNN-T graph trained on Russian *and* English — the default, so
//!     mixed-language meetings stop being transliterated into Cyrillic.
//!
//! The Russian-only variants all have distinct file names and coexist in `models/gigaam/`.
//! The bilingual export reuses the RNN-T file names verbatim, so it gets its own
//! [`GigaamVariant::subdir`].

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
    /// Bilingual (Russian + English) e2e-RNN-T fp32 — the default. Architecturally identical
    /// to [`Self::E2eRnntFp32`] (64 mel features, 768-wide conformer encoder, 320-wide
    /// prediction net, 1025 classes with blank 1024), so it decodes through the same
    /// [`crate::gigaam_engine::rnnt::RnntModel`]; only the weights and vocabulary differ.
    E2eRnntEnRu,
}

pub enum DecodeKind {
    Ctc,
    Rnnt,
}

impl Default for GigaamVariant {
    fn default() -> Self {
        // Bilingual RNN-T fp32 (2026-08-05): same architecture and precision as the
        // Russian-only fp32 default it replaces, but it transcribes English instead of
        // transliterating it. Existing installs keep whatever they already downloaded
        // (see `commands::read_selected`) — only fresh ones fetch this.
        GigaamVariant::E2eRnntEnRu
    }
}

impl GigaamVariant {
    pub const ALL: [GigaamVariant; 6] = [
        GigaamVariant::E2eCtcInt8,
        GigaamVariant::E2eCtcFp32,
        GigaamVariant::E2eRnntInt8,
        GigaamVariant::E2eRnntFp32,
        GigaamVariant::E2eRnntAne,
        GigaamVariant::E2eRnntEnRu,
    ];

    /// Stable id persisted to disk and exchanged with the frontend.
    pub fn id(self) -> &'static str {
        match self {
            GigaamVariant::E2eCtcInt8 => "e2e-ctc-int8",
            GigaamVariant::E2eCtcFp32 => "e2e-ctc-fp32",
            GigaamVariant::E2eRnntInt8 => "e2e-rnnt-int8",
            GigaamVariant::E2eRnntFp32 => "e2e-rnnt-fp32",
            GigaamVariant::E2eRnntAne => "e2e-rnnt-ane",
            GigaamVariant::E2eRnntEnRu => "e2e-rnnt-en-ru",
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
            GigaamVariant::E2eRnntFp32 => "e2e-RNN-T · Russian only · fp32",
            GigaamVariant::E2eRnntAne => {
                "e2e-RNN-T · Russian only · Neural Engine (fastest on Apple Silicon)"
            }
            GigaamVariant::E2eRnntEnRu => "e2e-RNN-T · Russian + English · fp32 (default)",
        }
    }

    /// Approximate total download size in MB — what the user is asked to transfer, which for
    /// an [`Self::archive_url`] variant is the whole archive, not just the files kept from it.
    pub fn approx_mb(self) -> u32 {
        match self {
            GigaamVariant::E2eCtcInt8 => 225,
            GigaamVariant::E2eCtcFp32 => 886,
            GigaamVariant::E2eRnntInt8 => 227,
            GigaamVariant::E2eRnntFp32 => 891,
            // 409 MB CoreML archive + the 6 MB decoder/joiner/vocab — no ONNX encoder.
            GigaamVariant::E2eRnntAne => 415,
            // The archive also carries int8 exports this variant doesn't keep, so the
            // download is larger than the ~892 MB that lands on disk.
            GigaamVariant::E2eRnntEnRu => 987,
        }
    }

    pub fn decode_kind(self) -> DecodeKind {
        match self {
            GigaamVariant::E2eCtcInt8 | GigaamVariant::E2eCtcFp32 => DecodeKind::Ctc,
            GigaamVariant::E2eRnntInt8
            | GigaamVariant::E2eRnntFp32
            | GigaamVariant::E2eRnntAne
            | GigaamVariant::E2eRnntEnRu => DecodeKind::Rnnt,
        }
    }

    /// Sub-directory of `models/gigaam/` holding this variant's files, when it cannot share
    /// the root. The bilingual export ships the RNN-T file names unchanged, so keeping it in
    /// the root would make it and [`Self::E2eRnntFp32`] overwrite each other — and worse,
    /// each would look "downloaded" while the other's weights were on disk.
    pub fn subdir(self) -> Option<&'static str> {
        match self {
            GigaamVariant::E2eRnntEnRu => Some("en_ru"),
            _ => None,
        }
    }

    /// Public Yandex Disk page for variants distributed as a single archive rather than as
    /// individual files. The direct download link is short-lived and has to be resolved
    /// through the Disk API at download time — see `commands::resolve_yandex_disk_href`.
    pub fn archive_url(self) -> Option<&'static str> {
        match self {
            GigaamVariant::E2eRnntEnRu => Some("https://disk.yandex.ru/d/Ty5v8ZWVvLbEjw"),
            _ => None,
        }
    }

    /// True when the encoder is the CoreML/Neural Engine model rather than ONNX. Such
    /// variants need [`Self::ane_asset`] downloaded and compiled next to the ONNX files.
    pub fn uses_ane_encoder(self) -> bool {
        matches!(self, GigaamVariant::E2eRnntAne)
    }

    /// True when the weights cover English as well as Russian. The Russian-only variants
    /// transliterate English speech into Cyrillic instead of recognizing it.
    pub fn is_bilingual(self) -> bool {
        matches!(self, GigaamVariant::E2eRnntEnRu)
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
            // Same names as E2eRnntFp32 — different weights, kept apart by `subdir`.
            GigaamVariant::E2eRnntFp32 | GigaamVariant::E2eRnntEnRu => vec![
                "v3_e2e_rnnt_encoder.onnx",
                "v3_e2e_rnnt_decoder.onnx",
                "v3_e2e_rnnt_joint.onnx",
            ],
            GigaamVariant::E2eRnntAne => {
                vec!["v3_e2e_rnnt_decoder.onnx", "v3_e2e_rnnt_joint.onnx"]
            }
        }
    }

    /// All model files that must be present locally / downloaded (vocab + ONNX). The
    /// CoreML encoder of an ANE variant comes from elsewhere and is tracked separately —
    /// see [`Self::ane_asset`] and `coreml::is_compiled_model_usable`.
    pub fn all_files(self) -> Vec<&'static str> {
        let mut files = vec![self.vocab_file()];
        files.extend(self.model_files());
        files
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<&str> = GigaamVariant::ALL.iter().map(|v| v.id()).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "two variants share an id");
    }

    /// Two variants may only share file names if they live in different directories —
    /// otherwise downloading one silently overwrites the other while both still report
    /// themselves as present.
    #[test]
    fn variants_sharing_file_names_have_distinct_dirs() {
        for a in GigaamVariant::ALL {
            for b in GigaamVariant::ALL {
                if a == b || a.subdir() != b.subdir() {
                    continue;
                }
                let overlap: Vec<_> = a
                    .all_files()
                    .into_iter()
                    .filter(|f| b.all_files().contains(f))
                    .collect();
                // The ANE variant deliberately reuses the fp32 decoder/joiner: identical
                // weights, one copy on disk. Any *encoder* overlap is a real collision.
                assert!(
                    overlap.iter().all(|f| !f.contains("encoder")),
                    "{} and {} share {overlap:?} in the same directory",
                    a.id(),
                    b.id()
                );
            }
        }
    }

    /// A variant that ships as one archive must be kept out of the shared root, since its
    /// file names are not guaranteed to be unique.
    #[test]
    fn archive_variants_have_their_own_subdir() {
        for v in GigaamVariant::ALL.iter().filter(|v| v.archive_url().is_some()) {
            assert!(v.subdir().is_some(), "{} needs a subdir", v.id());
        }
    }
}
