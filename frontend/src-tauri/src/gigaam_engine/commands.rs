//! GigaAM model management: variant select / download / status / startup load. Files live
//! in `app_data_dir/models/gigaam/`; the selected variant is persisted to
//! `selected_variant.txt` so startup reloads it. Variants differ in decoder (CTC vs
//! RNN-T), precision (int8 vs fp32) and — on Apple Silicon — where the encoder runs (ONNX
//! on the CPU vs CoreML on the Neural Engine) — see `variant.rs`.
//!
//! The Neural Engine variant is the one download that isn't a plain file fetch: its encoder
//! is a zipped CoreML `.mlpackage` from a GitHub release that has to be unpacked and
//! compiled locally before it can load — see [`ensure_ane_model`].

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
/// CoreML/Neural Engine encoder releases — an fp16 conversion of the same weights.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const ANE_BASE: &str = "https://github.com/IsaacClarke2/gigaam-v3-coreml/releases/download/v3.0";
static DOWNLOAD_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static DOWNLOAD_PROGRESS: Mutex<Option<DownloadProgress>> = Mutex::new(None);

/// The variants offered in the UI. Narrowed to RNN-T fp32 only (2026-07-20): it is the
/// measured-best variant, and offering int8/CTC alternatives only produced confusing
/// quality differences between installs. The other variants in [`GigaamVariant::ALL`]
/// remain loadable for A/B research (`load_global` accepts any of them).
///
/// Apple Silicon additionally gets the Neural Engine variant — same weights and same
/// transcripts, an order of magnitude less CPU. It is a separate entry rather than a flag on
/// the fp32 one because it is a different download (a CoreML encoder instead of the ONNX
/// one) and must stay off Intel Macs and every other platform.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const OFFERED_VARIANTS: [GigaamVariant; 2] =
    [GigaamVariant::E2eRnntAne, GigaamVariant::E2eRnntFp32];
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
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

/// True when every file the variant needs is present on disk — including the compiled
/// CoreML encoder, for variants that run on the Neural Engine.
fn variant_present(dir: &Path, v: GigaamVariant) -> bool {
    v.all_files().iter().all(|f| dir.join(f).exists()) && ane_model_present(dir, v)
}

/// True when this build and OS can run the CoreML encoder at all.
///
/// The converted model targets macOS 14 (iOS 17 ops, `specificationVersion` 8), so an older
/// system cannot load it — worth knowing *before* fetching 409 MB. An unreadable OS version
/// counts as supported: the load error is a better failure than a false refusal.
fn ane_supported() -> bool {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        macos_major_version().map(|major| major >= 14).unwrap_or(true)
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        false
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn macos_major_version() -> Option<u32> {
    sysinfo::System::os_version()?
        .split('.')
        .next()?
        .parse()
        .ok()
}

/// True when the variant either needs no CoreML encoder, or has a usable compiled one.
/// Always false off Apple Silicon: such a variant can never become usable there.
fn ane_model_present(dir: &Path, v: GigaamVariant) -> bool {
    if !v.uses_ane_encoder() {
        return true;
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        super::coreml::is_compiled_model_usable(&super::ane_model_dir(dir))
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        let _ = dir;
        false
    }
}

#[derive(serde::Serialize, Clone)]
struct DownloadProgress {
    file: String,
    downloaded: u64,
    total: u64,
    percent: u8,
    /// `downloading` | `extracting` | `compiling`. The last two have no byte counts (they
    /// are local CPU work), so the UI shows them as indeterminate steps.
    stage: &'static str,
}

/// Publish a step that has no byte progress of its own (unpacking, CoreML compilation).
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn publish_stage<R: Runtime>(app: &AppHandle<R>, file: &str, stage: &'static str) {
    let progress = DownloadProgress {
        file: file.to_string(),
        downloaded: 0,
        total: 0,
        percent: 0,
        stage,
    };
    if let Ok(mut current) = DOWNLOAD_PROGRESS.lock() {
        *current = Some(progress.clone());
    }
    let _ = app.emit("gigaam-download-progress", progress);
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

async fn download_file<R: Runtime>(
    app: &AppHandle<R>,
    url: &str,
    dest: &Path,
    label: &str,
) -> Result<(), String> {
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
                    stage: "downloading",
                };
                if let Ok(mut current) = DOWNLOAD_PROGRESS.lock() {
                    *current = Some(progress.clone());
                }
                let _ = app.emit("gigaam-download-progress", progress);
            }
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
    Ok(())
}

/// Fetch the CoreML encoder archive, unpack the `.mlpackage` inside it, and compile it to
/// the `.mlmodelc` CoreML loads. No-op for variants with no [`GigaamVariant::ane_asset`],
/// and for an already-compiled model.
///
/// Compilation happens here, on the user's machine, through CoreML's own compiler — see
/// [`super::coreml::compile_model`]. The archive and the unpacked package together are
/// ~800 MB and are deleted as soon as the ~423 MB `.mlmodelc` exists.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
async fn ensure_ane_model<R: Runtime>(
    app: &AppHandle<R>,
    dir: &Path,
    v: GigaamVariant,
) -> Result<(), String> {
    let Some(asset) = v.ane_asset() else {
        return Ok(());
    };
    let dest = super::ane_model_dir(dir);
    if super::coreml::is_compiled_model_usable(&dest) {
        return Ok(());
    }
    if !ane_supported() {
        return Err(
            "The Neural Engine model needs an Apple Silicon Mac on macOS 14 or newer".to_string(),
        );
    }

    let work = dir.join("ane");
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let archive = work.join(asset);
    download_file(app, &format!("{ANE_BASE}/{asset}"), &archive, asset).await?;

    publish_stage(app, asset, "extracting");
    let staging = work.join("staging");
    let archive_path = archive.clone();
    let package = tokio::task::spawn_blocking(move || extract_mlpackage(&archive_path, &staging))
        .await
        .map_err(|e| e.to_string())??;

    publish_stage(app, super::coreml::COMPILED_DIR_NAME, "compiling");
    let dest_path = dest.clone();
    tokio::task::spawn_blocking(move || {
        super::coreml::compile_model(&package, &dest_path).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    let _ = std::fs::remove_file(&archive);
    let _ = std::fs::remove_dir_all(work.join("staging"));
    Ok(())
}

/// Unpack a `*.mlpackage.zip` into `staging` and return the `.mlpackage` directory inside it.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn extract_mlpackage(archive: &Path, staging: &Path) -> Result<PathBuf, String> {
    let _ = std::fs::remove_dir_all(staging);
    std::fs::create_dir_all(staging).map_err(|e| e.to_string())?;
    let file = std::fs::File::open(archive).map_err(|e| format!("open {}: {e}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|e| format!("read {}: {e}", archive.display()))?;
    zip.extract(staging)
        .map_err(|e| format!("unpack {}: {e}", archive.display()))?;
    find_mlpackage(staging, 0)
        .ok_or_else(|| format!("no .mlpackage inside {}", archive.display()))
}

/// Depth-limited search for the `.mlpackage` directory in an unpacked archive (the release
/// zips nest it one level down, and macOS zips carry a `__MACOSX` sibling).
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn find_mlpackage(dir: &Path, depth: usize) -> Option<PathBuf> {
    if depth > 4 {
        return None;
    }
    for entry in std::fs::read_dir(dir).ok()? {
        let entry = entry.ok()?;
        if !entry.file_type().ok()?.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("__MACOSX") {
            continue;
        }
        if name.ends_with(".mlpackage") {
            return Some(entry.path());
        }
        if let Some(found) = find_mlpackage(&entry.path(), depth + 1) {
            return Some(found);
        }
    }
    None
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
                "neural_engine": v.uses_ane_encoder(),
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
        // Apple Silicon on macOS 14+ only — the UI uses this to explain why the Neural
        // Engine option is (or isn't) usable.
        "neural_engine_supported": ane_supported(),
    }))
}

/// Persist a variant selection. If its files are already present, load it immediately
/// (and emit `gigaam-ready`); otherwise the frontend will prompt a download. The
/// previously loaded model is left running until the new one is ready.
#[tauri::command]
pub async fn gigaam_select_variant<R: Runtime>(
    app: AppHandle<R>,
    variant: String,
) -> Result<(), String> {
    if DOWNLOAD_IN_PROGRESS.load(Ordering::SeqCst) {
        return Err("Wait for the current GigaAM model download to finish".to_string());
    }
    let v = GigaamVariant::from_id(&variant)
        .ok_or_else(|| format!("unknown GigaAM variant: {variant}"))?;
    if !OFFERED_VARIANTS.contains(&v) {
        return Err(format!(
            "GigaAM variant {variant} is not offered in this build"
        ));
    }
    if v.uses_ane_encoder() && !ane_supported() {
        return Err(
            "The Neural Engine model needs an Apple Silicon Mac on macOS 14 or newer".to_string(),
        );
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
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        ensure_ane_model(&app, &dir, v).await?;
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        if v.uses_ane_encoder() {
            return Err(format!(
                "GigaAM variant {} needs the Apple Neural Engine, which this build does not support",
                v.id()
            ));
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

    /// The Neural Engine variant is only offered where it can actually run.
    #[test]
    fn ane_variant_is_offered_on_apple_silicon_only() {
        let offered = OFFERED_VARIANTS.iter().any(|v| v.uses_ane_encoder());
        assert_eq!(
            offered,
            cfg!(all(target_os = "macos", target_arch = "aarch64"))
        );
        // The default must stay a variant this build can load.
        assert!(OFFERED_VARIANTS.contains(&GigaamVariant::default()));
    }

    /// Unpack a real release archive and find the `.mlpackage` inside it. Ignored; run with:
    ///   GIGAAM_ANE_ZIP=<gigaam-v3-encoder-ane.mlpackage.zip> \
    ///   cargo test --lib gigaam_engine::commands::tests::extracts_mlpackage -- --ignored --nocapture
    #[test]
    #[ignore]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn extracts_mlpackage() {
        let Ok(zip) = std::env::var("GIGAAM_ANE_ZIP") else {
            return;
        };
        let staging = std::env::temp_dir().join("meetily-ane-extract-test");
        let package = extract_mlpackage(Path::new(&zip), &staging).expect("unpack the archive");
        eprintln!("unpacked → {}", package.display());
        assert!(package.extension().is_some_and(|e| e == "mlpackage"));
        assert!(
            package.join("Manifest.json").exists(),
            "not an .mlpackage: no Manifest.json"
        );
        let _ = std::fs::remove_dir_all(&staging);
    }
}
