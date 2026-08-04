//! GigaAM v3 (Russian ASR) transcription engine — ONNX via `ort`, mirroring the
//! `parakeet_engine`/`embedder` global-instance pattern. Supports e2e-CTC and e2e-RNN-T
//! variants (int8/fp32) selectable at runtime for A/B quality testing — see `variant.rs`.
//! On Apple Silicon one more variant runs the RNN-T encoder on the Neural Engine instead of
//! the CPU (`coreml`), keeping the decoder and joiner on `ort`.
//! Output is punctuated, capitalized Russian.

pub mod commands;
/// Encoder on the Apple Neural Engine — Apple Silicon only (see [`coreml`]).
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub mod coreml;
pub mod featurizer;
pub mod model;
pub mod rnnt;
pub mod variant;

use std::path::PathBuf;
use std::sync::Mutex;

use model::CtcModel;
use rnnt::RnntModel;
pub use rnnt::TimedWord;
use variant::{DecodeKind, GigaamVariant};

/// The loaded model, dispatched by decode kind.
pub enum LoadedModel {
    Ctc(CtcModel),
    Rnnt(RnntModel),
}

impl LoadedModel {
    fn transcribe(&mut self, waveform: &[f32]) -> anyhow::Result<String> {
        match self {
            LoadedModel::Ctc(m) => m.transcribe(waveform),
            LoadedModel::Rnnt(m) => m.transcribe(waveform),
        }
    }

    /// Word-level transcription. `Ok(None)` when the loaded variant doesn't expose
    /// per-token timing (CTC — derivable from frame argmax, just not implemented).
    fn transcribe_with_words(
        &mut self,
        waveform: &[f32],
    ) -> anyhow::Result<Option<Vec<TimedWord>>> {
        match self {
            LoadedModel::Ctc(_) => Ok(None),
            LoadedModel::Rnnt(m) => m.transcribe_with_words(waveform).map(Some),
        }
    }
}

// Process-wide GigaAM model + which variant is loaded. `std::sync::Mutex` (const-init);
// the model is Send; inference is synchronous so the lock is never held across an await
// (async callers use spawn_blocking).
static ENGINE: Mutex<Option<LoadedModel>> = Mutex::new(None);
static LOADED_VARIANT: Mutex<Option<GigaamVariant>> = Mutex::new(None);

/// Load `variant`'s files from `dir` into the global slot, replacing any previous model.
///
/// The previous model stays loaded if this fails: the new one is fully built before the slot
/// is reassigned. That matters most for the Neural Engine variant, whose load can fail on an
/// unsupported OS or a half-compiled CoreML model.
pub fn load_global(variant: GigaamVariant, dir: PathBuf) -> anyhow::Result<()> {
    let model = match variant.decode_kind() {
        DecodeKind::Ctc => {
            let files = variant.model_files();
            LoadedModel::Ctc(CtcModel::load(
                &dir.join(files[0]),
                &dir.join(variant.vocab_file()),
            )?)
        }
        // Encoder on the Neural Engine: `model_files` holds only decoder + joiner, and the
        // encoder is the compiled CoreML model in `dir`.
        DecodeKind::Rnnt if variant.uses_ane_encoder() => {
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            {
                let f = variant.model_files();
                LoadedModel::Rnnt(RnntModel::load_with_ane_encoder(
                    &ane_model_dir(&dir),
                    &dir.join(f[0]),
                    &dir.join(f[1]),
                    &dir.join(variant.vocab_file()),
                )?)
            }
            #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
            {
                anyhow::bail!(
                    "GigaAM variant {} needs the Apple Neural Engine, which this build does not support",
                    variant.id()
                )
            }
        }
        DecodeKind::Rnnt => {
            let f = variant.model_files();
            LoadedModel::Rnnt(RnntModel::load(
                &dir.join(f[0]),
                &dir.join(f[1]),
                &dir.join(f[2]),
                &dir.join(variant.vocab_file()),
            )?)
        }
    };
    *ENGINE.lock().unwrap() = Some(model);
    *LOADED_VARIANT.lock().unwrap() = Some(variant);
    log::info!("GigaAM {} model loaded", variant.id());
    Ok(())
}

/// Where the compiled CoreML encoder lives inside the GigaAM model dir. Kept out of the dir
/// root because it is a directory (`.mlmodelc`), not a file like every other model asset.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub fn ane_model_dir(dir: &std::path::Path) -> PathBuf {
    dir.join("ane").join(coreml::COMPILED_DIR_NAME)
}

pub fn is_loaded() -> bool {
    ENGINE.lock().map(|g| g.is_some()).unwrap_or(false)
}

/// The variant currently loaded, if any (may differ from the persisted selection until a
/// newly-selected variant finishes downloading/loading).
pub fn loaded_variant() -> Option<GigaamVariant> {
    *LOADED_VARIANT.lock().unwrap()
}

/// Provenance/display label for the model that actually transcribes: derived from the
/// loaded variant (e.g. "gigaam-v3-e2e-rnnt-fp32"). Historically this was hardcoded to
/// "gigaam-v3-e2e-ctc" everywhere, so logs and metadata claimed the CTC variant no
/// matter what was running.
pub fn model_label() -> String {
    match loaded_variant() {
        Some(v) => format!("gigaam-v3-{}", v.id()),
        None => "gigaam-v3".to_string(),
    }
}

pub fn unload() {
    if let Ok(mut g) = ENGINE.lock() {
        *g = None;
    }
    if let Ok(mut v) = LOADED_VARIANT.lock() {
        *v = None;
    }
}

/// Transcribe a 16 kHz mono waveform on a blocking thread. `None` if no model is loaded.
pub async fn transcribe(waveform: Vec<f32>) -> Option<Result<String, String>> {
    if !is_loaded() {
        return None;
    }
    tokio::task::spawn_blocking(move || {
        let mut guard = ENGINE.lock().unwrap();
        guard
            .as_mut()
            .map(|m| m.transcribe(&waveform).map_err(|e| e.to_string()))
    })
    .await
    .ok()
    .flatten()
}

/// Word-level transcription on a blocking thread. Outer `None` = no model loaded;
/// inner `Ok(None)` = the loaded variant has no per-word timing (caller should fall
/// back to [`transcribe`]).
pub async fn transcribe_with_words(
    waveform: Vec<f32>,
) -> Option<Result<Option<Vec<TimedWord>>, String>> {
    if !is_loaded() {
        return None;
    }
    tokio::task::spawn_blocking(move || {
        let mut guard = ENGINE.lock().unwrap();
        guard.as_mut().map(|m| {
            m.transcribe_with_words(&waveform)
                .map_err(|e| e.to_string())
        })
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The path an install actually takes for the Neural Engine variant: `load_global` over a
    /// real model directory (what `init_gigaam_at_startup` does), then a word-timed
    /// transcription through the global engine (what the refinement pass does).
    ///
    /// Ignored; run with a `models/gigaam` dir holding the RNN-T ONNX files plus
    /// `ane/encoder-ane.mlmodelc`:
    ///   GIGAAM_MODEL_DIR=<dir> GIGAAM_TEST_WAV=<clip> \
    ///   cargo test --lib gigaam_engine::tests::ane_variant_loads_and_transcribes -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    async fn ane_variant_loads_and_transcribes() {
        let (Ok(dir), Ok(wav)) = (
            std::env::var("GIGAAM_MODEL_DIR"),
            std::env::var("GIGAAM_TEST_WAV"),
        ) else {
            return;
        };

        let dir = PathBuf::from(dir);
        tokio::task::spawn_blocking(move || load_global(GigaamVariant::E2eRnntAne, dir))
            .await
            .unwrap()
            .expect("load the Neural Engine variant");
        assert!(is_loaded());
        assert_eq!(loaded_variant(), Some(GigaamVariant::E2eRnntAne));
        assert_eq!(model_label(), "gigaam-v3-e2e-rnnt-ane");

        let decoded =
            crate::audio::decoder::decode_audio_file(std::path::Path::new(&wav)).expect("decode");
        let samples = decoded.to_whisper_format();
        let clip = samples[..(20 * 16_000).min(samples.len())].to_vec();

        let words = transcribe_with_words(clip)
            .await
            .expect("engine loaded")
            .expect("transcribe")
            .expect("RNN-T reports word timing");
        assert!(!words.is_empty(), "no words decoded");
        for w in words.iter().take(8) {
            eprintln!("{:6}-{:6} ms  {}", w.start_ms, w.end_ms, w.text);
        }
        unload();
    }
}
