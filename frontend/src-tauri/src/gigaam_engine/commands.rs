//! GigaAM model management: variant select / download / status / startup load. Files live
//! in `app_data_dir/models/gigaam/`; the selected variant is persisted to
//! `selected_variant.txt` so startup reloads it. Variants differ in decoder (CTC vs
//! RNN-T), precision (int8 vs fp32) and — on Apple Silicon — where the encoder runs (ONNX
//! on the CPU vs CoreML on the Neural Engine) — see `variant.rs`.
//!
//! Two variants don't download as plain per-file fetches:
//!   - the bilingual RU+EN default ships as one Yandex Disk archive whose direct link has to
//!     be resolved through the Disk API first — see [`download_archive_variant`];
//!   - the Neural Engine encoder is a zipped CoreML `.mlpackage` from a GitHub release that
//!     has to be unpacked and compiled locally before it can load — see [`ensure_ane_model`].

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

/// The variants offered in the UI. Narrowed to RNN-T fp32 (2026-07-20): it is the
/// measured-best variant, and offering int8/CTC alternatives only produced confusing
/// quality differences between installs. The other variants in [`GigaamVariant::ALL`]
/// remain loadable for A/B research (`load_global` accepts any of them).
///
/// The bilingual RU+EN export leads the list and is the default (2026-08-05) — same
/// architecture and precision as the Russian-only fp32 model, but it transcribes English
/// rather than transliterating it. The Russian-only entries stay so existing installs keep
/// working without a fresh ~1 GB download.
///
/// Apple Silicon additionally gets the Neural Engine variant — same weights and same
/// transcripts as Russian-only fp32, an order of magnitude less CPU. It is a separate entry
/// rather than a flag on the fp32 one because it is a different download (a CoreML encoder
/// instead of the ONNX one) and must stay off Intel Macs and every other platform.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const OFFERED_VARIANTS: [GigaamVariant; 3] = [
    GigaamVariant::E2eRnntEnRu,
    GigaamVariant::E2eRnntAne,
    GigaamVariant::E2eRnntFp32,
];
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
const OFFERED_VARIANTS: [GigaamVariant; 2] =
    [GigaamVariant::E2eRnntEnRu, GigaamVariant::E2eRnntFp32];

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
/// retired variant (e.g. a legacy e2e-ctc-int8 selection) resolves to the default —
/// such installs see "Not downloaded" once and fetch the supported model.
///
/// With no marker at all, an offered variant that is already on disk wins over the default.
/// Only `gigaam_select_variant` writes the marker, so every install that just pressed
/// "Download" has none; without this, changing the default would strand those users on a
/// fresh ~1 GB download of a model they effectively already have.
fn read_selected(dir: &Path) -> GigaamVariant {
    if let Some(v) = std::fs::read_to_string(variant_marker(dir))
        .ok()
        .and_then(|s| GigaamVariant::from_id(s.trim()))
        .filter(|v| OFFERED_VARIANTS.contains(v))
    {
        return v;
    }
    let default = GigaamVariant::default();
    if variant_present(dir, default) {
        return default;
    }
    OFFERED_VARIANTS
        .into_iter()
        .find(|v| variant_present(dir, *v))
        .unwrap_or(default)
}

fn write_selected(dir: &Path, v: GigaamVariant) -> Result<(), String> {
    std::fs::write(variant_marker(dir), v.id()).map_err(|e| e.to_string())
}

/// True when every file the variant needs is present in its own directory — including the
/// compiled CoreML encoder, for variants that run on the Neural Engine.
fn variant_present(dir: &Path, v: GigaamVariant) -> bool {
    let files = super::variant_dir(dir, v);
    v.all_files().iter().all(|f| files.join(f).exists()) && ane_model_present(dir, v)
}

/// True when this build and OS can run the CoreML encoder at all.
///
/// The converted model targets macOS 14 (iOS 17 ops, `specificationVersion` 8), so an older
/// system cannot load it — worth knowing *before* fetching 409 MB. An unreadable OS version
/// counts as supported: the load error is a better failure than a false refusal.
fn ane_supported() -> bool {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        macos_major_version()
            .map(|major| major >= 14)
            .unwrap_or(true)
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

/// Whether a finished transfer is intact.
///
/// `content_length` is what the response itself promised (0 when it promised nothing, as a
/// chunked response does); `expected` is a size known out of band — for an archive, the
/// Disk API's own figure. Both are checked, because a response with no `Content-Length`
/// would otherwise go entirely unverified, which is precisely how a truncated ~1 GB archive
/// slips through to fail later at unpack.
fn verify_transfer(
    label: &str,
    downloaded: u64,
    content_length: u64,
    expected: Option<u64>,
) -> Result<(), String> {
    if let Some(expected) = expected {
        if downloaded != expected {
            return Err(format!(
                "download {label}: got {downloaded} bytes, expected {expected}"
            ));
        }
        return Ok(());
    }
    if content_length > 0 && downloaded != content_length {
        return Err(format!(
            "download {label}: connection ended after {downloaded} of {content_length} bytes"
        ));
    }
    // Nothing to compare against, so catch at least the unmistakable failure: no model asset
    // is ever empty.
    if downloaded == 0 {
        return Err(format!("download {label}: the server sent no data"));
    }
    Ok(())
}

/// Download `url` to `dest`. `expected` is the size the file should have, when known
/// independently of the response headers — see [`verify_transfer`].
async fn download_file<R: Runtime>(
    app: &AppHandle<R>,
    url: &str,
    dest: &Path,
    label: &str,
    expected: Option<u64>,
) -> Result<(), String> {
    if dest.exists() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
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
    let content_length = resp.content_length().unwrap_or(0);
    // Percentages can still be reported for a chunked response when the size is known
    // out of band.
    let total = if content_length > 0 {
        content_length
    } else {
        expected.unwrap_or(0)
    };

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
    drop(file);
    // A connection dropped mid-stream yields a short file with no error of its own. Renaming
    // it into place would make the model look installed and fail at load (or, for an archive,
    // at unpack) with a far less obvious message.
    if let Err(e) = verify_transfer(label, downloaded, content_length, expected) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
    Ok(())
}

/// Turn a public Yandex Disk page URL into a direct download link.
///
/// Public-link downloads need this indirection: the page URL serves HTML, and the real
/// `downloader.disk.yandex.ru` link is signed and short-lived, so it has to be minted right
/// before the transfer rather than hardcoded. The API needs no credentials.
async fn resolve_yandex_disk_href(public_url: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct DownloadLink {
        href: String,
    }
    let resp = reqwest::Client::new()
        .get("https://cloud-api.yandex.net/v1/disk/public/resources/download")
        .query(&[("public_key", public_url)])
        .send()
        .await
        .map_err(|e| format!("resolve the model download link: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "resolve the model download link: HTTP {}",
            resp.status()
        ));
    }
    let link: DownloadLink = resp
        .json()
        .await
        .map_err(|e| format!("resolve the model download link: {e}"))?;
    Ok(link.href)
}

/// The size the Disk API reports for a public file, used to verify the transfer.
///
/// Deliberately fetched rather than hardcoded: the archive is a file in someone's Disk folder
/// and can be replaced with a newer export, which a pinned constant would turn into a hard
/// failure. `None` on any problem — an unverifiable download is still better than no
/// download, and [`verify_transfer`] falls back to the response's own `Content-Length`.
async fn yandex_disk_file_size(public_url: &str) -> Option<u64> {
    #[derive(serde::Deserialize)]
    struct PublicResource {
        size: Option<u64>,
    }
    let resp = reqwest::Client::new()
        .get("https://cloud-api.yandex.net/v1/disk/public/resources")
        .query(&[("public_key", public_url)])
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<PublicResource>().await.ok()?.size
}

/// Fetch an [`GigaamVariant::archive_url`] variant: resolve the link, download the archive
/// into the variant's own directory, unpack just the files the variant needs, and delete the
/// archive.
///
/// The archive carries exports this variant doesn't use (int8 copies of the same graphs), so
/// unpacking is selective — the full contents would be ~230 MB of dead weight. Peak disk use
/// is still archive + extracted files at once, about 1.9 GB.
async fn download_archive_variant<R: Runtime>(
    app: &AppHandle<R>,
    dir: &Path,
    v: GigaamVariant,
) -> Result<(), String> {
    let Some(public_url) = v.archive_url() else {
        return Ok(());
    };
    let target = super::variant_dir(dir, v);
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;

    let archive = target.join("model.zip");
    if !archive.exists() {
        let expected = yandex_disk_file_size(public_url).await;
        let href = resolve_yandex_disk_href(public_url).await?;
        let label = format!("{}.zip", v.id());
        download_file(app, &href, &archive, &label, expected).await?;
    }

    publish_stage(app, "model.zip", "extracting");
    let wanted: Vec<String> = v.all_files().iter().map(|f| f.to_string()).collect();
    let (archive_path, target_path) = (archive.clone(), target.clone());
    let extracted = tokio::task::spawn_blocking(move || {
        extract_named_entries(&archive_path, &target_path, &wanted)
    })
    .await
    .map_err(|e| e.to_string())?;

    match extracted {
        Ok(()) => {
            let _ = std::fs::remove_file(&archive);
            Ok(())
        }
        // A bad archive can never unpack, so drop it and let the retry refetch. A local I/O
        // failure (out of disk, most likely) says nothing about the archive — keep it, so
        // retrying after freeing space doesn't mean another ~1 GB download.
        Err(ExtractError::BadArchive(e)) => {
            let _ = std::fs::remove_file(&archive);
            Err(e)
        }
        Err(ExtractError::Io(e)) => Err(e),
    }
}

/// Why an extraction failed — the caller uses this to decide whether the archive is worth
/// keeping for a retry.
#[derive(Debug)]
enum ExtractError {
    /// The archive is unreadable, corrupt, or doesn't contain the expected files.
    BadArchive(String),
    /// Reading or writing on this machine failed (no space left, permissions).
    Io(String),
}

/// Unpack exactly `wanted` (matched on the entry's file name, so any archive root works)
/// from `archive` into `target`. Errors if the archive is missing any of them.
fn extract_named_entries(
    archive: &Path,
    target: &Path,
    wanted: &[String],
) -> Result<(), ExtractError> {
    let file = std::fs::File::open(archive)
        .map_err(|e| ExtractError::BadArchive(format!("open {}: {e}", archive.display())))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|e| ExtractError::BadArchive(format!("read {}: {e}", archive.display())))?;

    use std::io::Write;
    let mut found: Vec<&String> = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| ExtractError::BadArchive(e.to_string()))?;
        // `enclosed_name` rejects absolute and `..` paths; we only keep the file name anyway.
        let Some(name) = entry
            .enclosed_name()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        else {
            continue;
        };
        let Some(want) = wanted.iter().find(|w| **w == name) else {
            continue;
        };
        let dest = target.join(want.as_str());
        let tmp = dest.with_extension("part");
        let mut out = std::fs::File::create(&tmp)
            .map_err(|e| ExtractError::Io(format!("create {}: {e}", tmp.display())))?;
        let copied = std::io::copy(&mut entry, &mut out)
            .and_then(|_| out.flush())
            .map_err(|e| {
                // A broken compressed stream surfaces as InvalidData; anything else (ENOSPC,
                // EACCES) is this machine's problem, not the archive's.
                let message = format!("unpack {name}: {e}");
                match e.kind() {
                    std::io::ErrorKind::InvalidData => ExtractError::BadArchive(message),
                    _ => ExtractError::Io(message),
                }
            });
        drop(out);
        if let Err(e) = copied {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        std::fs::rename(&tmp, &dest)
            .map_err(|e| ExtractError::Io(format!("finalize {}: {e}", dest.display())))?;
        found.push(want);
    }

    let missing: Vec<&str> = wanted
        .iter()
        .filter(|w| !found.contains(w))
        .map(|w| w.as_str())
        .collect();
    if !missing.is_empty() {
        return Err(ExtractError::BadArchive(format!(
            "the model archive is missing {} — it may have been replaced upstream",
            missing.join(", ")
        )));
    }
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
    download_file(app, &format!("{ANE_BASE}/{asset}"), &archive, asset, None).await?;

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
    let file =
        std::fs::File::open(archive).map_err(|e| format!("open {}: {e}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|e| format!("read {}: {e}", archive.display()))?;
    zip.extract(staging)
        .map_err(|e| format!("unpack {}: {e}", archive.display()))?;
    find_mlpackage(staging, 0).ok_or_else(|| format!("no .mlpackage inside {}", archive.display()))
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
                "bilingual": v.is_bilingual(),
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
        if v.archive_url().is_some() {
            download_archive_variant(&app, &dir, v).await?;
        } else {
            for f in v.all_files() {
                download_file(&app, &format!("{HF_BASE}/{f}"), &dir.join(f), f, None).await?;
            }
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

    /// A short transfer must be rejected however the size became known — including the case
    /// the response promised nothing, which is how a chunked CDN reply arrives.
    #[test]
    fn truncated_transfers_are_rejected() {
        // Content-Length known and matched / short.
        assert!(verify_transfer("m", 100, 100, None).is_ok());
        assert!(verify_transfer("m", 60, 100, None).is_err());

        // No Content-Length, but the Disk API told us the size.
        assert!(verify_transfer("m", 100, 0, Some(100)).is_ok());
        let err = verify_transfer("m", 60, 0, Some(100)).expect_err("short archive");
        assert!(err.contains("expected 100"), "{err}");

        // An out-of-band size also catches a *wrong* file, not just a short one.
        assert!(verify_transfer("m", 140, 140, Some(100)).is_err());

        // Nothing to compare against: an empty body is still unmistakably wrong.
        assert!(verify_transfer("m", 1, 0, None).is_ok());
        assert!(verify_transfer("m", 0, 0, None).is_err());
    }

    /// Touch every file a variant needs, as a completed download would.
    fn install(dir: &Path, v: GigaamVariant) {
        let files = super::super::variant_dir(dir, v);
        std::fs::create_dir_all(&files).unwrap();
        for f in v.all_files() {
            std::fs::write(files.join(f), b"x").unwrap();
        }
    }

    /// An install that pressed "Download" without ever opening the variant dropdown has no
    /// marker file. Changing the default must not point such an install at a model it would
    /// have to download from scratch.
    #[test]
    fn markerless_install_keeps_the_variant_it_already_has() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), GigaamVariant::E2eRnntFp32);
        assert_eq!(read_selected(dir.path()), GigaamVariant::E2eRnntFp32);
    }

    /// With nothing on disk, a fresh install gets the default.
    #[test]
    fn empty_install_gets_the_default_variant() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_selected(dir.path()), GigaamVariant::default());
    }

    /// The default wins over an older variant once it is installed too.
    #[test]
    fn default_wins_when_both_are_present() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), GigaamVariant::E2eRnntFp32);
        install(dir.path(), GigaamVariant::default());
        assert_eq!(read_selected(dir.path()), GigaamVariant::default());
    }

    /// An explicit selection always wins, and the bilingual variant's files must not be
    /// mistaken for the Russian-only ones that share their names.
    #[test]
    fn subdir_variant_is_tracked_separately_from_the_root_one() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), GigaamVariant::E2eRnntEnRu);
        assert!(variant_present(dir.path(), GigaamVariant::E2eRnntEnRu));
        assert!(
            !variant_present(dir.path(), GigaamVariant::E2eRnntFp32),
            "the bilingual download must not make the Russian-only model look installed"
        );
        write_selected(dir.path(), GigaamVariant::E2eRnntFp32).unwrap();
        assert_eq!(read_selected(dir.path()), GigaamVariant::E2eRnntFp32);
    }

    /// Extraction pulls the wanted files out of a nested archive root and ignores the rest.
    #[test]
    fn extracts_only_the_wanted_entries() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("model.zip");
        {
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            for (name, body) in [
                ("en_ru_onnx/v3_e2e_rnnt_vocab.txt", &b"vocab"[..]),
                ("en_ru_onnx/v3_e2e_rnnt_encoder.onnx", &b"enc"[..]),
                ("en_ru_onnx/v3_e2e_rnnt_decoder.onnx", &b"dec"[..]),
                ("en_ru_onnx/v3_e2e_rnnt_joint.onnx", &b"joint"[..]),
                ("en_ru_onnx/v3_e2e_rnnt_encoder.int8.onnx", &b"skip"[..]),
                ("en_ru_onnx/config.json", &b"skip"[..]),
            ] {
                zip.start_file(name, opts).unwrap();
                zip.write_all(body).unwrap();
            }
            zip.finish().unwrap();
        }

        let target = dir.path().join("en_ru");
        std::fs::create_dir_all(&target).unwrap();
        let wanted: Vec<String> = GigaamVariant::E2eRnntEnRu
            .all_files()
            .iter()
            .map(|f| f.to_string())
            .collect();
        extract_named_entries(&archive, &target, &wanted).expect("unpack the archive");

        for f in GigaamVariant::E2eRnntEnRu.all_files() {
            assert!(target.join(f).exists(), "{f} was not unpacked");
        }
        assert!(!target.join("config.json").exists());
        assert!(!target.join("v3_e2e_rnnt_encoder.int8.onnx").exists());
        assert_eq!(
            std::fs::read(target.join("v3_e2e_rnnt_encoder.onnx")).unwrap(),
            b"enc",
            "the fp32 encoder must not be overwritten by the int8 entry"
        );
    }

    /// An archive missing a needed file fails loudly rather than leaving a half-install that
    /// only breaks later, at model load.
    #[test]
    fn extraction_reports_missing_entries() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("model.zip");
        {
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
            zip.start_file(
                "en_ru_onnx/v3_e2e_rnnt_vocab.txt",
                zip::write::FileOptions::<()>::default(),
            )
            .unwrap();
            zip.write_all(b"vocab").unwrap();
            zip.finish().unwrap();
        }
        let wanted: Vec<String> = GigaamVariant::E2eRnntEnRu
            .all_files()
            .iter()
            .map(|f| f.to_string())
            .collect();
        let err = extract_named_entries(&archive, dir.path(), &wanted)
            .expect_err("a truncated archive must be rejected");
        let ExtractError::BadArchive(message) = err else {
            panic!("a missing entry is the archive's fault, not this machine's");
        };
        assert!(message.contains("v3_e2e_rnnt_encoder.onnx"), "{message}");
    }

    /// Unpack the real published archive and check the sizes of what lands on disk. Ignored;
    /// run with a downloaded copy:
    ///   GIGAAM_ENRU_ZIP=<en_ru_onnx.zip> \
    ///   cargo test --lib gigaam_engine::commands::tests::extracts_the_real_archive -- --ignored --nocapture
    #[test]
    #[ignore]
    fn extracts_the_real_archive() {
        let Ok(zip) = std::env::var("GIGAAM_ENRU_ZIP") else {
            return;
        };
        let v = GigaamVariant::E2eRnntEnRu;
        let dir = tempfile::tempdir().unwrap();
        let wanted: Vec<String> = v.all_files().iter().map(|f| f.to_string()).collect();
        extract_named_entries(Path::new(&zip), dir.path(), &wanted).expect("unpack");
        let mut total = 0u64;
        for f in v.all_files() {
            let len = std::fs::metadata(dir.path().join(f)).unwrap().len();
            eprintln!("{f}: {len} bytes");
            total += len;
        }
        eprintln!("total {} MB", total / 1_000_000);
        // The published fp32 set is ~892 MB; a wildly different total means the archive
        // changed shape and `approx_mb` is lying to the user.
        assert!((850_000_000..950_000_000).contains(&total), "total {total}");
    }

    /// The Yandex Disk public API mints the real download link and reports the size the
    /// transfer is checked against. Network-gated:
    ///   cargo test --lib gigaam_engine::commands::tests::resolves_the_archive_link -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn resolves_the_archive_link() {
        let url = GigaamVariant::E2eRnntEnRu.archive_url().unwrap();
        let href = resolve_yandex_disk_href(url).await.expect("resolve");
        let size = yandex_disk_file_size(url).await.expect("size");
        eprintln!("{size} bytes\n{href}");
        assert!(href.starts_with("https://"));
        // Sanity-check against the advertised ~987 MB, so a swapped-out archive is visible.
        assert!((900_000_000..1_100_000_000).contains(&size), "size {size}");
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
