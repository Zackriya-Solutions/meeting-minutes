//! Diarization model files: location, status and verified download.
use super::Cancelled;
use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime};
use tokio::io::AsyncWriteExt;

/// A download that delivers no data for this long is abandoned.
const STALL_TIMEOUT: Duration = Duration::from_secs(30);

pub struct ModelFile {
    pub file_name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size_bytes: u64,
}

/// pyannote segmentation-3.0 (MIT), ONNX export, pinned to an immutable revision.
pub const SEGMENTATION: ModelFile = ModelFile {
    file_name: "segmentation-3.0.onnx",
    url: "https://huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0/resolve/9403a6902bb58e3d5ae8c7e77c3422de279db2e0/model.onnx",
    sha256: "220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079",
    size_bytes: 5_992_913,
};

/// 3D-Speaker CAM++ trained on VoxCeleb (Apache-2.0), ONNX export, pinned to an immutable revision.
/// Update `embedding::PATCHED_SHA256` together with this pin, or every load fails as a damaged model.
pub const EMBEDDING: ModelFile = ModelFile {
    file_name: "campplus-voxceleb.onnx",
    url: "https://huggingface.co/csukuangfj/speaker-embedding-models/resolve/0743f301363dec56491a490f6d6cbc9d67f9a3bf/3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx",
    sha256: "357a834f702b80161e5b981182c038e18553c1f2ca752ed6cec2052365d4129b",
    size_bytes: 29_596_978,
};

pub const MODEL_FILES: [&ModelFile; 2] = [&SEGMENTATION, &EMBEDDING];

static MODELS_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
static DOWNLOAD_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub fn set_models_directory<R: Runtime>(app: &AppHandle<R>) {
    match app.path().app_data_dir() {
        Ok(dir) => {
            let dir = dir.join("models").join("diarization");
            log::info!("Diarization models directory set to: {}", dir.display());
            *MODELS_DIR.lock().unwrap_or_else(|e| e.into_inner()) = Some(dir);
        }
        Err(e) => log::error!("Failed to resolve app data dir for diarization models: {}", e),
    }
}

pub fn models_directory() -> Result<PathBuf> {
    MODELS_DIR
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| anyhow!("Diarization models directory is not configured"))
}

pub(crate) fn is_installed(dir: &Path, file: &ModelFile) -> bool {
    std::fs::metadata(dir.join(file.file_name))
        .map(|m| m.len() == file.size_bytes)
        .unwrap_or(false)
}

#[derive(Serialize, Clone, Debug)]
pub struct ModelsStatus {
    pub installed: bool,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub directory: String,
}

pub fn status(dir: &Path) -> ModelsStatus {
    let total_bytes = MODEL_FILES.iter().map(|f| f.size_bytes).sum();
    let downloaded_bytes = MODEL_FILES
        .iter()
        .filter(|f| is_installed(dir, f))
        .map(|f| f.size_bytes)
        .sum();
    ModelsStatus {
        installed: MODEL_FILES.iter().all(|f| is_installed(dir, f)),
        total_bytes,
        downloaded_bytes,
        directory: dir.display().to_string(),
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub percent: u32,
}

/// reqwest errors already include their causes in their message; keep only that message
/// (without the URL) so `{:#}` further up prints each cause once.
fn request_error(e: reqwest::Error) -> anyhow::Error {
    if e.is_connect() || e.is_timeout() {
        anyhow!("Couldn't reach the model server. Check your internet connection. ({})", e.without_url())
    } else {
        anyhow!("{}", e.without_url())
    }
}

/// Download one file to `<name>.part`, verify size and SHA-256, then rename into place.
/// Stops with `Cancelled` when `cancelled` returns true, and fails when no data arrives for
/// STALL_TIMEOUT. The `.part` file is removed on any failure.
pub(crate) async fn download_file(
    client: &reqwest::Client,
    file: &ModelFile,
    dir: &Path,
    on_bytes: &(dyn Fn(u64) + Send + Sync),
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Result<()> {
    tokio::fs::create_dir_all(dir).await?;
    let part = dir.join(format!("{}.part", file.file_name));
    let result: Result<()> = async {
        let response = client
            .get(file.url)
            .header(reqwest::header::USER_AGENT, "Meetily")
            .send()
            .await
            .map_err(request_error)?
            .error_for_status()
            .map_err(request_error)?;
        let mut out = tokio::fs::File::create(&part).await?;
        let mut hasher = Sha256::new();
        let mut received = 0u64;
        let mut stream = response.bytes_stream();
        loop {
            if cancelled() {
                return Err(Cancelled.into());
            }
            let next = tokio::time::timeout(STALL_TIMEOUT, stream.next())
                .await
                .map_err(|_| anyhow!("Download of {} stalled: no data for {} s", file.file_name, STALL_TIMEOUT.as_secs()))?;
            let Some(chunk) = next else { break };
            let chunk = chunk.map_err(request_error)?;
            hasher.update(&chunk);
            out.write_all(&chunk).await?;
            received += chunk.len() as u64;
            on_bytes(received);
        }
        out.flush().await?;
        drop(out);
        let digest = format!("{:x}", hasher.finalize());
        if received != file.size_bytes || digest != file.sha256 {
            return Err(anyhow!(
                "Downloaded {} failed verification (got {} bytes, sha256 {})",
                file.file_name, received, digest
            ));
        }
        tokio::fs::rename(&part, dir.join(file.file_name)).await?;
        Ok(())
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&part).await;
    }
    result
}

/// Download any missing model files. Concurrent callers wait for the first download;
/// `cancelled` is checked while waiting and between chunks.
pub async fn ensure_models(
    dir: &Path,
    on_progress: impl Fn(DownloadProgress) + Send + Sync,
    cancelled: impl Fn() -> bool + Send + Sync,
) -> Result<()> {
    let _guard = loop {
        if let Ok(guard) = DOWNLOAD_LOCK.try_lock() {
            break guard;
        }
        if cancelled() {
            return Err(Cancelled.into());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    if status(dir).installed {
        // Another caller finished the download while this one waited.
        return Ok(());
    }
    let client = reqwest::Client::builder()
        .user_agent("Meetily")
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(3600))
        .build()?;
    let total: u64 = MODEL_FILES.iter().map(|f| f.size_bytes).sum();
    let mut done: u64 = MODEL_FILES.iter().filter(|f| is_installed(dir, f)).map(|f| f.size_bytes).sum();
    // Progress is reported once per percent, not once per network chunk.
    let last_percent = AtomicU32::new(u32::MAX);
    for file in MODEL_FILES {
        if is_installed(dir, file) {
            continue;
        }
        log::info!("Downloading diarization model {}", file.file_name);
        let base = done;
        let report = |received: u64| {
            let downloaded = base + received;
            let percent = ((downloaded as f64 / total.max(1) as f64) * 100.0).min(100.0) as u32;
            if last_percent.swap(percent, Ordering::Relaxed) != percent {
                on_progress(DownloadProgress { downloaded_bytes: downloaded, total_bytes: total, percent });
            }
        };
        download_file(&client, file, dir, &report, &cancelled)
            .await
            .with_context(|| format!("Failed to download {}", file.file_name))?;
        done += file.size_bytes;
    }
    Ok(())
}

pub fn delete_models(dir: &Path) -> Result<()> {
    for file in MODEL_FILES {
        let path = dir.join(file.file_name);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Serves `body` once per connection over plain HTTP; returns the base URL.
    async fn serve(body: Vec<u8>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0u8; 1024];
                let _ = socket.read(&mut buf).await;
                let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                socket.write_all(head.as_bytes()).await.unwrap();
                socket.write_all(&body).await.unwrap();
            }
        });
        format!("http://{addr}")
    }

    fn leak(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }

    /// Talks to the loopback test server directly, ignoring any HTTP_PROXY/ALL_PROXY in the environment.
    fn local_client() -> reqwest::Client {
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

    #[test]
    fn pinned_model_files_are_complete() {
        for f in MODEL_FILES {
            assert_eq!(f.sha256.len(), 64, "{} needs its SHA-256", f.file_name);
            assert!(f.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(f.size_bytes > 0);
        }
    }

    #[tokio::test]
    async fn verified_download_installs_the_file() {
        let body = b"model-bytes".to_vec();
        let base = serve(body.clone()).await;
        let file = ModelFile {
            file_name: "m.onnx",
            url: leak(format!("{base}/m.onnx")),
            sha256: leak(format!("{:x}", Sha256::digest(&body))),
            size_bytes: body.len() as u64,
        };
        let dir = tempfile::tempdir().unwrap();
        download_file(&local_client(), &file, dir.path(), &|_| {}, &|| false).await.unwrap();
        assert_eq!(std::fs::read(dir.path().join("m.onnx")).unwrap(), body);
        assert!(is_installed(dir.path(), &file));
    }

    #[tokio::test]
    async fn checksum_mismatch_leaves_nothing_installed() {
        let base = serve(b"tampered".to_vec()).await;
        let file = ModelFile {
            file_name: "m.onnx",
            url: leak(format!("{base}/m.onnx")),
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
            size_bytes: 8,
        };
        let dir = tempfile::tempdir().unwrap();
        assert!(download_file(&local_client(), &file, dir.path(), &|_| {}, &|| false).await.is_err());
        assert!(!dir.path().join("m.onnx").exists());
        assert!(!dir.path().join("m.onnx.part").exists());
    }

    #[test]
    fn status_reports_missing_models() {
        let dir = tempfile::tempdir().unwrap();
        let s = status(dir.path());
        assert!(!s.installed);
        assert_eq!(s.total_bytes, SEGMENTATION.size_bytes + EMBEDDING.size_bytes);
        assert_eq!(s.downloaded_bytes, 0);
    }

    #[tokio::test]
    async fn cancelled_download_leaves_nothing_installed() {
        let body = b"model-bytes".to_vec();
        let base = serve(body.clone()).await;
        let file = ModelFile {
            file_name: "m.onnx",
            url: leak(format!("{base}/m.onnx")),
            sha256: leak(format!("{:x}", Sha256::digest(&body))),
            size_bytes: body.len() as u64,
        };
        let dir = tempfile::tempdir().unwrap();
        let err = download_file(&local_client(), &file, dir.path(), &|_| {}, &|| true).await.unwrap_err();
        assert!(err.is::<crate::diarization::Cancelled>());
        assert!(!dir.path().join("m.onnx").exists());
        assert!(!dir.path().join("m.onnx.part").exists());
    }

    #[tokio::test]
    async fn connection_errors_name_each_cause_once() {
        let file = ModelFile { file_name: "m.onnx", url: "http://127.0.0.1:1/m.onnx", sha256: "", size_bytes: 1 };
        let dir = tempfile::tempdir().unwrap();
        let err = download_file(&local_client(), &file, dir.path(), &|_| {}, &|| false).await.unwrap_err();
        let msg = format!("{err:#}");
        assert_eq!(msg.matches("error trying to connect").count(), 1, "{msg}");
    }
}
