//! GigaAM v3 (Russian ASR) transcription engine — ONNX via `ort`, mirroring the
//! `parakeet_engine`/`embedder` global-instance pattern. Uses the e2e-CTC model
//! (`istupakov/gigaam-v3-onnx`): punctuated, capitalized Russian output.

pub mod commands;
pub mod featurizer;
pub mod model;

use std::path::PathBuf;
use std::sync::Mutex;

use model::GigaamModel;

// Process-wide GigaAM model. `std::sync::Mutex` (const-init); model is Send; inference is
// synchronous so the lock is never held across an await (async callers use spawn_blocking).
static ENGINE: Mutex<Option<GigaamModel>> = Mutex::new(None);

/// Load the GigaAM v3 e2e-CTC model from `model_path` (+ `vocab_path`) into the global
/// slot, replacing any previous model.
pub fn load_global(model_path: PathBuf, vocab_path: PathBuf) -> anyhow::Result<()> {
    let m = GigaamModel::load(&model_path, &vocab_path)?;
    *ENGINE.lock().unwrap() = Some(m);
    log::info!("GigaAM v3 e2e CTC model loaded");
    Ok(())
}

pub fn is_loaded() -> bool {
    ENGINE.lock().map(|g| g.is_some()).unwrap_or(false)
}

pub fn unload() {
    if let Ok(mut g) = ENGINE.lock() {
        *g = None;
    }
}

/// Transcribe a 16 kHz mono waveform on a blocking thread. `None` if no model is loaded.
pub async fn transcribe(waveform: Vec<f32>) -> Option<Result<String, String>> {
    if !is_loaded() {
        return None;
    }
    tokio::task::spawn_blocking(move || {
        let mut guard = ENGINE.lock().unwrap();
        guard.as_mut().map(|m| m.transcribe(&waveform).map_err(|e| e.to_string()))
    })
    .await
    .ok()
    .flatten()
}
