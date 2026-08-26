// diarization/models.rs
//
// Model location and download for speaker identification.
// Mirrors the parakeet_engine download pattern: stream from a stable URL
// into <app_data>/models/diarization/, .tmp + rename for atomicity,
// progress emitted as Tauri events.

use futures_util::StreamExt;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// WeSpeaker CAM++ speaker-embedding model (Apache-2.0, exported to ONNX by
/// the sherpa-onnx project). ~28 MB. Input: fbank [1, T, 80]; output: [1, 192].
/// NOTE: "recongition" is the canonical (misspelled) sherpa-onnx release tag.
pub const EMBEDDING_MODEL_FILENAME: &str = "wespeaker_en_voxceleb_CAM++.onnx";
pub const EMBEDDING_MODEL_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/wespeaker_en_voxceleb_CAM%2B%2B.onnx";

pub fn models_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    Ok(app_data_dir.join("models").join("diarization"))
}

pub fn embedding_model_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(models_dir(app)?.join(EMBEDDING_MODEL_FILENAME))
}

pub fn is_embedding_model_present<R: Runtime>(app: &AppHandle<R>) -> bool {
    embedding_model_path(app)
        .map(|p| p.exists() && std::fs::metadata(&p).map(|m| m.len() > 1_000_000).unwrap_or(false))
        .unwrap_or(false)
}

/// Download the embedding model, emitting `diarization-model-download-progress`
/// events with { downloaded_bytes, total_bytes, percent }.
pub async fn download_embedding_model<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let dir = models_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create models dir: {}", e))?;

    let final_path = dir.join(EMBEDDING_MODEL_FILENAME);
    if is_embedding_model_present(app) {
        log::info!("Diarization embedding model already present at {}", final_path.display());
        return Ok(());
    }
    let tmp_path = dir.join(format!("{}.tmp", EMBEDDING_MODEL_FILENAME));

    log::info!("Downloading diarization embedding model from {}", EMBEDDING_MODEL_URL);
    let client = reqwest::Client::new();
    let response = client
        .get(EMBEDDING_MODEL_URL)
        .send()
        .await
        .map_err(|e| format!("Download request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Download failed with HTTP {}", response.status()));
    }

    let total_bytes = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut last_emitted_percent: i64 = -1;

    let mut file = tokio::fs::File::create(&tmp_path)
        .await
        .map_err(|e| format!("Failed to create temp file: {}", e))?;

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download stream error: {}", e))?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
            .await
            .map_err(|e| format!("Failed to write model file: {}", e))?;
        downloaded += chunk.len() as u64;

        let percent = if total_bytes > 0 {
            (downloaded * 100 / total_bytes) as i64
        } else {
            0
        };
        if percent != last_emitted_percent {
            last_emitted_percent = percent;
            let _ = app.emit(
                "diarization-model-download-progress",
                serde_json::json!({
                    "downloaded_bytes": downloaded,
                    "total_bytes": total_bytes,
                    "percent": percent,
                }),
            );
        }
    }
    tokio::io::AsyncWriteExt::flush(&mut file)
        .await
        .map_err(|e| format!("Failed to flush model file: {}", e))?;
    drop(file);

    std::fs::rename(&tmp_path, &final_path)
        .map_err(|e| format!("Failed to finalize model file: {}", e))?;

    log::info!(
        "Diarization embedding model downloaded to {} ({} bytes)",
        final_path.display(),
        downloaded
    );
    Ok(())
}
