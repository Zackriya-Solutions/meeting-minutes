use anyhow::{anyhow, Context, Result};
use bzip2::read::BzDecoder;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tar::Archive;
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub const MODEL_ID: &str = "sherpa-v1";
const BACKEND_ID: &str = "sherpa-pyannote3-eres2net";
const SEGMENTATION_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2";
const SEGMENTATION_SHA256: &str =
    "24615ee884c897d9d2ba09bb4d30da6bb1b15e685065962db5b02e76e4996488";
const SEGMENTATION_MODEL_SHA256: &str =
    "d582f4b4c6b48205de7e0643c57df0df5615a3c176189be3fc461e9d18827b5d";
const SEGMENTATION_DOWNLOAD_SIZE: u64 = 6_958_444;
const EMBEDDING_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx";
const EMBEDDING_SHA256: &str = "1a331345f04805badbb495c775a6ddffcdd1a732567d5ec8b3d5749e3c7a5e4b";
const EMBEDDING_DOWNLOAD_SIZE: u64 = 39_593_761;
const TOTAL_DOWNLOAD_SIZE: u64 = SEGMENTATION_DOWNLOAD_SIZE + EMBEDDING_DOWNLOAD_SIZE;

#[derive(Debug, Clone)]
pub struct SpeakerModelPaths {
    pub root: PathBuf,
    pub segmentation: PathBuf,
    pub embedding: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerModelManifest {
    pub id: String,
    pub version: u32,
    pub backend: String,
    pub segmentation_source: String,
    pub segmentation_sha256: String,
    pub segmentation_model_sha256: String,
    pub embedding_source: String,
    pub embedding_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpeakerModelStatus {
    pub id: String,
    pub status: String,
    pub size_mb: f64,
    pub path: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DownloadProgressEvent {
    model_id: String,
    progress: u8,
    downloaded_mb: f64,
    total_mb: f64,
    status: String,
}

pub fn model_root<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_data_dir()
        .context("Unable to resolve app data directory")?
        .join("models")
        .join("speaker-diarization")
        .join(MODEL_ID))
}

pub fn paths_for_root(root: PathBuf) -> SpeakerModelPaths {
    SpeakerModelPaths {
        segmentation: root.join("segmentation").join("model.int8.onnx"),
        embedding: root.join("embedding").join("3dspeaker_eres2net.onnx"),
        root,
    }
}

pub fn installed_model_paths<R: Runtime>(app: &AppHandle<R>) -> Result<Option<SpeakerModelPaths>> {
    let paths = paths_for_root(model_root(app)?);
    if validate_installation(&paths).is_ok() {
        Ok(Some(paths))
    } else {
        Ok(None)
    }
}

pub fn get_status<R: Runtime>(app: &AppHandle<R>) -> Result<SpeakerModelStatus> {
    let paths = paths_for_root(model_root(app)?);
    let validation = validate_installation(&paths);
    let status = if validation.is_ok() {
        "available"
    } else if paths.root.exists() {
        "corrupt"
    } else {
        "missing"
    };

    Ok(SpeakerModelStatus {
        id: MODEL_ID.to_string(),
        status: status.to_string(),
        size_mb: TOTAL_DOWNLOAD_SIZE as f64 / 1_000_000.0,
        path: paths.root.to_string_lossy().to_string(),
        error: validation.err().map(|error| error.to_string()),
    })
}

pub async fn download_model<R: Runtime>(app: AppHandle<R>) -> Result<()> {
    let final_root = model_root(&app)?;
    let parent = final_root
        .parent()
        .ok_or_else(|| anyhow!("Invalid speaker model directory"))?
        .to_path_buf();
    tokio::fs::create_dir_all(&parent).await?;

    let staging = parent.join(format!(".{}.download", MODEL_ID));
    if staging.exists() {
        tokio::fs::remove_dir_all(&staging).await?;
    }
    tokio::fs::create_dir_all(&staging).await?;

    let result = async {
        let archive_part = staging.join("segmentation.tar.bz2.part");
        download_file(
            &app,
            SEGMENTATION_URL,
            &archive_part,
            0,
            TOTAL_DOWNLOAD_SIZE,
        )
        .await?;
        verify_sha256(&archive_part, SEGMENTATION_SHA256)?;

        let embedding_dir = staging.join("embedding");
        tokio::fs::create_dir_all(&embedding_dir).await?;
        let embedding_part = embedding_dir.join("3dspeaker_eres2net.onnx.part");
        download_file(
            &app,
            EMBEDDING_URL,
            &embedding_part,
            SEGMENTATION_DOWNLOAD_SIZE,
            TOTAL_DOWNLOAD_SIZE,
        )
        .await?;
        verify_sha256(&embedding_part, EMBEDDING_SHA256)?;
        tokio::fs::rename(
            &embedding_part,
            embedding_dir.join("3dspeaker_eres2net.onnx"),
        )
        .await?;

        let staging_for_extract = staging.clone();
        let archive_for_extract = archive_part.clone();
        tokio::task::spawn_blocking(move || {
            extract_segmentation_archive(&archive_for_extract, &staging_for_extract)
        })
        .await
        .map_err(|error| anyhow!("Segmentation extraction task failed: {error}"))??;
        tokio::fs::remove_file(&archive_part).await?;

        let manifest = SpeakerModelManifest {
            id: MODEL_ID.to_string(),
            version: 1,
            backend: BACKEND_ID.to_string(),
            segmentation_source: SEGMENTATION_URL.to_string(),
            segmentation_sha256: SEGMENTATION_SHA256.to_string(),
            segmentation_model_sha256: SEGMENTATION_MODEL_SHA256.to_string(),
            embedding_source: EMBEDDING_URL.to_string(),
            embedding_sha256: EMBEDDING_SHA256.to_string(),
        };
        tokio::fs::write(
            staging.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest)?,
        )
        .await?;

        let staging_paths = paths_for_root(staging.clone());
        validate_installation(&staging_paths)?;

        let backup_root = parent.join(format!(".{}.backup", MODEL_ID));
        if backup_root.exists() {
            tokio::fs::remove_dir_all(&backup_root).await?;
        }
        if final_root.exists() {
            tokio::fs::rename(&final_root, &backup_root).await?;
        }
        if let Err(error) = tokio::fs::rename(&staging, &final_root).await {
            if backup_root.exists() {
                let _ = tokio::fs::rename(&backup_root, &final_root).await;
            }
            return Err(error.into());
        }
        if backup_root.exists() {
            tokio::fs::remove_dir_all(&backup_root).await?;
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;

    match result {
        Ok(()) => {
            let _ = app.emit(
                "speaker-diarization-model-download-complete",
                serde_json::json!({ "model_id": MODEL_ID }),
            );
            Ok(())
        }
        Err(error) => {
            let _ = tokio::fs::remove_dir_all(&staging).await;
            let _ = app.emit(
                "speaker-diarization-model-download-error",
                serde_json::json!({ "model_id": MODEL_ID, "error": error.to_string() }),
            );
            Err(error)
        }
    }
}

pub async fn delete_model<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    let root = model_root(app)?;
    if root.exists() {
        tokio::fs::remove_dir_all(root).await?;
    }
    Ok(())
}

async fn download_file<R: Runtime>(
    app: &AppHandle<R>,
    url: &str,
    destination: &Path,
    completed_before: u64,
    total_bytes: u64,
) -> Result<()> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(1800))
        .build()?;
    let response = client.get(url).send().await?.error_for_status()?;
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(destination).await?;
    let mut downloaded = 0u64;
    let mut last_progress = u8::MAX;

    use tokio::io::AsyncWriteExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        let aggregate = completed_before + downloaded;
        let progress = ((aggregate as f64 / total_bytes as f64) * 100.0)
            .floor()
            .clamp(0.0, 99.0) as u8;
        if progress != last_progress {
            last_progress = progress;
            let _ = app.emit(
                "speaker-diarization-model-download-progress",
                DownloadProgressEvent {
                    model_id: MODEL_ID.to_string(),
                    progress,
                    downloaded_mb: aggregate as f64 / 1_000_000.0,
                    total_mb: total_bytes as f64 / 1_000_000.0,
                    status: "downloading".to_string(),
                },
            );
        }
    }
    file.flush().await?;
    Ok(())
}

fn verify_sha256(path: &Path, expected: &str) -> Result<()> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        return Err(anyhow!(
            "Checksum mismatch for {}: expected {}, got {}",
            path.display(),
            expected,
            actual
        ));
    }
    Ok(())
}

fn extract_segmentation_archive(archive_path: &Path, staging: &Path) -> Result<()> {
    let segmentation_dir = staging.join("segmentation");
    let licenses_dir = staging.join("licenses");
    std::fs::create_dir_all(&segmentation_dir)?;
    std::fs::create_dir_all(&licenses_dir)?;

    let decoder = BzDecoder::new(File::open(archive_path)?);
    let mut archive = Archive::new(decoder);
    let mut model_found = false;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        let filename = path.file_name().and_then(|name| name.to_str());
        let destination = match filename {
            Some("model.int8.onnx") => {
                model_found = true;
                Some(segmentation_dir.join("model.int8.onnx"))
            }
            Some("LICENSE") => Some(licenses_dir.join("pyannote-segmentation-MIT.txt")),
            Some("README.md") => Some(licenses_dir.join("pyannote-segmentation-README.md")),
            _ => None,
        };
        if let Some(destination) = destination {
            let mut output = File::create(destination)?;
            std::io::copy(&mut entry, &mut output)?;
            output.flush()?;
        }
    }
    if !model_found {
        return Err(anyhow!(
            "Segmentation archive did not contain model.int8.onnx"
        ));
    }

    std::fs::write(
        licenses_dir.join("3D-Speaker-Apache-2.0.txt"),
        "3D-Speaker is licensed under the Apache License 2.0.\nhttps://github.com/modelscope/3D-Speaker\n",
    )?;
    std::fs::write(
        licenses_dir.join("sherpa-onnx-Apache-2.0.txt"),
        "sherpa-onnx is licensed under the Apache License 2.0.\nhttps://github.com/k2-fsa/sherpa-onnx\n",
    )?;
    Ok(())
}

fn validate_installation(paths: &SpeakerModelPaths) -> Result<()> {
    if !paths.segmentation.is_file() {
        return Err(anyhow!("Segmentation model is missing"));
    }
    if !paths.embedding.is_file() {
        return Err(anyhow!("Speaker embedding model is missing"));
    }
    if std::fs::metadata(&paths.segmentation)?.len() < 1_000_000 {
        return Err(anyhow!("Segmentation model is incomplete"));
    }
    if std::fs::metadata(&paths.embedding)?.len() < 35_000_000 {
        return Err(anyhow!("Speaker embedding model is incomplete"));
    }
    verify_sha256(&paths.segmentation, SEGMENTATION_MODEL_SHA256)?;
    verify_sha256(&paths.embedding, EMBEDDING_SHA256)?;
    let manifest_path = paths.root.join("manifest.json");
    let manifest: SpeakerModelManifest = serde_json::from_slice(&std::fs::read(&manifest_path)?)?;
    if manifest.id != MODEL_ID
        || manifest.version != 1
        || manifest.backend != BACKEND_ID
        || manifest.segmentation_source != SEGMENTATION_URL
        || manifest.segmentation_sha256 != SEGMENTATION_SHA256
        || manifest.segmentation_model_sha256 != SEGMENTATION_MODEL_SHA256
        || manifest.embedding_source != EMBEDDING_URL
        || manifest.embedding_sha256 != EMBEDDING_SHA256
    {
        return Err(anyhow!("Unsupported speaker model manifest"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_paths_are_stable() {
        let paths = paths_for_root(PathBuf::from("/tmp/speaker-model"));
        assert!(paths.segmentation.ends_with("segmentation/model.int8.onnx"));
        assert!(paths
            .embedding
            .ends_with("embedding/3dspeaker_eres2net.onnx"));
    }
}
