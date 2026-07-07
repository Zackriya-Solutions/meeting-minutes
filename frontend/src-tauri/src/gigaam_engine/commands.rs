//! GigaAM model management: download / status / startup load. Files live in
//! `app_data_dir/models/gigaam/`. Default = the int8 e2e-CTC export from
//! `istupakov/gigaam-v3-onnx` (~224 MB): fast, small, punctuated Russian output.

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use tauri::{AppHandle, Emitter, Manager, Runtime};

const HF_BASE: &str = "https://huggingface.co/istupakov/gigaam-v3-onnx/resolve/main";
pub const MODEL_FILE: &str = "v3_e2e_ctc.int8.onnx";
pub const VOCAB_FILE: &str = "v3_e2e_ctc_vocab.txt";

fn gigaam_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models")
        .join("gigaam");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn model_present(dir: &Path) -> bool {
    dir.join(MODEL_FILE).exists() && dir.join(VOCAB_FILE).exists()
}

#[derive(serde::Serialize, Clone)]
struct DownloadProgress {
    file: String,
    downloaded: u64,
    total: u64,
    percent: u8,
}

async fn download_file<R: Runtime>(app: &AppHandle<R>, url: &str, dest: &Path, label: &str) -> Result<(), String> {
    if dest.exists() {
        return Ok(());
    }
    let tmp = dest.with_extension("part");
    let resp = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("download {label}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("download {label}: HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);

    use std::io::Write;
    let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut last_pct: u8 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("download {label}: {e}"))?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if total > 0 {
            let pct = ((downloaded.saturating_mul(100)) / total) as u8;
            if pct != last_pct {
                last_pct = pct;
                let _ = app.emit(
                    "gigaam-download-progress",
                    DownloadProgress { file: label.to_string(), downloaded, total, percent: pct },
                );
            }
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
    Ok(())
}

/// Transcribe 16 kHz mono f32 samples with the loaded GigaAM model (live path — the
/// frontend calls this when the transcript provider is `gigaam`). Mirrors
/// `parakeet_transcribe_audio`.
#[tauri::command]
pub async fn gigaam_transcribe_audio(audio_data: Vec<f32>) -> Result<String, String> {
    match super::transcribe(audio_data).await {
        Some(result) => result,
        None => Err("GigaAM model not loaded".to_string()),
    }
}

#[tauri::command]
pub async fn gigaam_status<R: Runtime>(app: AppHandle<R>) -> Result<serde_json::Value, String> {
    let dir = gigaam_dir(&app)?;
    Ok(serde_json::json!({
        "model": "gigaam-v3-e2e-ctc",
        "model_present": model_present(&dir),
        "loaded": super::is_loaded(),
    }))
}

/// Download the GigaAM v3 e2e-CTC model (vocab + int8 ONNX) and load it.
#[tauri::command]
pub async fn gigaam_download_model<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let dir = gigaam_dir(&app)?;
    download_file(&app, &format!("{HF_BASE}/{VOCAB_FILE}"), &dir.join(VOCAB_FILE), VOCAB_FILE).await?;
    download_file(&app, &format!("{HF_BASE}/{MODEL_FILE}"), &dir.join(MODEL_FILE), MODEL_FILE).await?;

    let model_path = dir.join(MODEL_FILE);
    let vocab_path = dir.join(VOCAB_FILE);
    tokio::task::spawn_blocking(move || super::load_global(model_path, vocab_path))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    let _ = app.emit("gigaam-ready", ());
    log::info!("GigaAM model downloaded and loaded");
    Ok(())
}

/// Load the GigaAM model at startup if its files are already present. Non-blocking.
pub async fn init_gigaam_at_startup<R: Runtime>(app: &AppHandle<R>) {
    let dir = match gigaam_dir(app) {
        Ok(d) => d,
        Err(e) => {
            log::warn!("could not resolve gigaam model dir: {e}");
            return;
        }
    };
    if model_present(&dir) {
        let model_path = dir.join(MODEL_FILE);
        let vocab_path = dir.join(VOCAB_FILE);
        let _ = tokio::task::spawn_blocking(move || {
            if let Err(e) = super::load_global(model_path, vocab_path) {
                log::warn!("failed to load GigaAM at startup: {e}");
            }
        })
        .await;
    }
}
