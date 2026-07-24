//! GigaAM model management: variant select / download / status / startup load. Files live
//! in `app_data_dir/models/gigaam/`; the selected variant is persisted to
//! `selected_variant.txt` so startup reloads it. Variants differ in decoder (CTC vs
//! RNN-T) and precision (int8 vs fp32) for A/B quality testing — see `variant.rs`.

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use futures_util::StreamExt;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::variant::GigaamVariant;

const HF_BASE: &str = "https://huggingface.co/istupakov/gigaam-v3-onnx/resolve/main";
static DOWNLOAD_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static DOWNLOAD_PROGRESS: Mutex<Option<DownloadProgress>> = Mutex::new(None);

/// The variants offered in the UI. Narrowed to RNN-T fp32 only (2026-07-20): it is the
/// measured-best variant, and offering int8/CTC alternatives only produced confusing
/// quality differences between installs. The other variants in [`GigaamVariant::ALL`]
/// remain loadable for A/B research (`load_global` accepts any of them).
const OFFERED_VARIANTS: [GigaamVariant; 1] = [GigaamVariant::E2eRnntFp32];

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

fn variant_marker(dir: &Path) -> PathBuf {
    dir.join("selected_variant.txt")
}

/// The persisted variant selection, clamped to the offered set: a marker pointing at a
/// retired variant (e.g. a legacy e2e-ctc-int8 selection) resolves to the default
/// RNN-T fp32 — such installs see "Not downloaded" once and fetch the supported model.
fn read_selected(dir: &Path) -> GigaamVariant {
    if let Some(v) = std::fs::read_to_string(variant_marker(dir))
        .ok()
        .and_then(|s| GigaamVariant::from_id(s.trim()))
        .filter(|v| OFFERED_VARIANTS.contains(v))
    {
        return v;
    }
    GigaamVariant::default()
}

fn write_selected(dir: &Path, v: GigaamVariant) -> Result<(), String> {
    std::fs::write(variant_marker(dir), v.id()).map_err(|e| e.to_string())
}

/// True when every file the variant needs is present on disk.
fn variant_present(dir: &Path, v: GigaamVariant) -> bool {
    v.all_files().iter().all(|f| dir.join(f).exists())
}

#[derive(serde::Serialize, Clone)]
struct DownloadProgress {
    file: String,
    downloaded: u64,
    total: u64,
    percent: u8,
}

struct DownloadGuard;

impl DownloadGuard {
    fn acquire() -> Result<Self, String> {
        DOWNLOAD_IN_PROGRESS
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "GigaAM model download is already in progress".to_string())?;
        if let Ok(mut progress) = DOWNLOAD_PROGRESS.lock() {
            *progress = None;
        }
        Ok(Self)
    }
}

impl Drop for DownloadGuard {
    fn drop(&mut self) {
        DOWNLOAD_IN_PROGRESS.store(false, Ordering::SeqCst);
        if let Ok(mut progress) = DOWNLOAD_PROGRESS.lock() {
            *progress = None;
        }
    }
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
                let progress = DownloadProgress {
                    file: label.to_string(),
                    downloaded,
                    total,
                    percent: pct,
                };
                if let Ok(mut current) = DOWNLOAD_PROGRESS.lock() {
                    *current = Some(progress.clone());
                }
                let _ = app.emit(
                    "gigaam-download-progress",
                    progress,
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
    let selected = read_selected(&dir);
    let variants: Vec<serde_json::Value> = OFFERED_VARIANTS
        .iter()
        .map(|v| {
            serde_json::json!({
                "id": v.id(),
                "label": v.label(),
                "size_mb": v.approx_mb(),
                "present": variant_present(&dir, *v),
            })
        })
        .collect();
    Ok(serde_json::json!({
        "selected": selected.id(),
        "model_present": variant_present(&dir, selected),
        "loaded": super::is_loaded(),
        "loaded_variant": super::loaded_variant().map(|v| v.id()),
        "downloading": DOWNLOAD_IN_PROGRESS.load(Ordering::SeqCst),
        "download_progress": DOWNLOAD_PROGRESS.lock().ok().and_then(|progress| progress.clone()),
        "variants": variants,
    }))
}

/// Persist a variant selection. If its files are already present, load it immediately
/// (and emit `gigaam-ready`); otherwise the frontend will prompt a download. The
/// previously loaded model is left running until the new one is ready.
#[tauri::command]
pub async fn gigaam_select_variant<R: Runtime>(app: AppHandle<R>, variant: String) -> Result<(), String> {
    if DOWNLOAD_IN_PROGRESS.load(Ordering::SeqCst) {
        return Err("Wait for the current GigaAM model download to finish".to_string());
    }
    let v = GigaamVariant::from_id(&variant).ok_or_else(|| format!("unknown GigaAM variant: {variant}"))?;
    if !OFFERED_VARIANTS.contains(&v) {
        return Err(format!("GigaAM variant {variant} is not offered in this build"));
    }
    let dir = gigaam_dir(&app)?;
    write_selected(&dir, v)?;
    if variant_present(&dir, v) {
        let d = dir.clone();
        tokio::task::spawn_blocking(move || super::load_global(v, d))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let _ = app.emit("gigaam-ready", ());
        log::info!("GigaAM variant switched to {}", v.id());
    }
    Ok(())
}

/// Download the currently-selected variant's files (vocab + ONNX) and load it.
#[tauri::command]
pub async fn gigaam_download_model<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let guard = DownloadGuard::acquire()?;
    let result = async {
        let dir = gigaam_dir(&app)?;
        let v = read_selected(&dir);
        for f in v.all_files() {
            download_file(&app, &format!("{HF_BASE}/{f}"), &dir.join(f), f).await?;
        }

        let d = dir.clone();
        tokio::task::spawn_blocking(move || super::load_global(v, d))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        Ok::<GigaamVariant, String>(v)
    }
    .await;

    // Clear the process-wide state before notifying remounted Settings components.
    drop(guard);
    match result {
        Ok(v) => {
            let _ = app.emit("gigaam-ready", ());
            log::info!("GigaAM {} downloaded and loaded", v.id());
            Ok(())
        }
        Err(error) => {
            let _ = app.emit("gigaam-download-error", error.clone());
            Err(error)
        }
    }
}

/// Load the selected GigaAM variant at startup if its files are already present.
/// Non-blocking.
pub async fn init_gigaam_at_startup<R: Runtime>(app: &AppHandle<R>) {
    let dir = match gigaam_dir(app) {
        Ok(d) => d,
        Err(e) => {
            log::warn!("could not resolve gigaam model dir: {e}");
            return;
        }
    };
    let v = read_selected(&dir);
    if variant_present(&dir, v) {
        let d = dir.clone();
        let _ = tokio::task::spawn_blocking(move || {
            if let Err(e) = super::load_global(v, d) {
                log::warn!("failed to load GigaAM at startup: {e}");
            }
        })
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_guard_rejects_duplicates_and_releases_state() {
        let first = DownloadGuard::acquire().expect("first download acquires the guard");
        assert!(DOWNLOAD_IN_PROGRESS.load(Ordering::SeqCst));
        assert!(
            DownloadGuard::acquire().is_err(),
            "a second download must be rejected"
        );

        drop(first);
        assert!(!DOWNLOAD_IN_PROGRESS.load(Ordering::SeqCst));
        let second = DownloadGuard::acquire().expect("guard is reusable after completion");
        drop(second);
    }
}
