use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const SENSE_VOICE_MODEL: &str = "sense-voice-small-int8";
pub const MODEL_REVISION: &str = "2365baeacb507f821a0c8120fcee3d484dba7a07";
pub const MODEL_BASE_URL: &str =
    "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve";
pub const MODEL_SIZE_BYTES: u64 = 239_549_806;
pub const REVISION_MARKER: &str = ".meetily-model-revision";

#[derive(Debug, Clone, Copy)]
pub struct ModelFile {
    pub relative_path: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

pub const MODEL_FILES: [ModelFile; 3] = [
    ModelFile {
        relative_path: "model.int8.onnx",
        size: 239_233_841,
        sha256: "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51",
    },
    ModelFile {
        relative_path: "tokens.txt",
        size: 315_894,
        sha256: "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc",
    },
    ModelFile {
        relative_path: "LICENSE",
        size: 71,
        sha256: "221c6df10b0931a5629adad671ea48fb7747e034c414b6d2bfa275bc3dd4ea17",
    },
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModelStatus {
    Available,
    Missing,
    Downloading { progress: u8 },
    Error(String),
    Corrupted { file_size: u64, expected_size: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub path: PathBuf,
    pub size_mb: u64,
    pub status: ModelStatus,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub percent: u8,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub downloaded_mb: f64,
    pub total_mb: f64,
    pub speed_mbps: f64,
}

pub fn model_info(models_dir: &Path) -> ModelInfo {
    let model_dir = models_dir.join(SENSE_VOICE_MODEL);
    ModelInfo {
        name: SENSE_VOICE_MODEL.to_string(),
        path: model_dir.clone(),
        size_mb: MODEL_SIZE_BYTES / 1_048_576,
        status: inspect_model(&model_dir),
        description: "Fast Mandarin, Cantonese, English, Japanese, and Korean recognition"
            .to_string(),
    }
}

pub fn inspect_model(model_dir: &Path) -> ModelStatus {
    let mut present_bytes = 0;
    let mut present_files = 0;

    for file in MODEL_FILES {
        let path = model_dir.join(file.relative_path);
        match std::fs::metadata(path) {
            Ok(metadata) => {
                present_files += 1;
                present_bytes += metadata.len();
                if metadata.len() != file.size {
                    return ModelStatus::Corrupted {
                        file_size: metadata.len(),
                        expected_size: file.size,
                    };
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return ModelStatus::Error(error.to_string()),
        }
    }

    if present_files == MODEL_FILES.len() && present_bytes == MODEL_SIZE_BYTES {
        match std::fs::read_to_string(model_dir.join(REVISION_MARKER)) {
            Ok(revision) if revision.trim() == MODEL_REVISION => ModelStatus::Available,
            _ => ModelStatus::Corrupted {
                file_size: present_bytes,
                expected_size: MODEL_SIZE_BYTES,
            },
        }
    } else if present_files == 0 {
        ModelStatus::Missing
    } else {
        ModelStatus::Corrupted {
            file_size: present_bytes,
            expected_size: MODEL_SIZE_BYTES,
        }
    }
}

pub fn verify_model_hashes(model_dir: &Path) -> Result<(), String> {
    for file in MODEL_FILES {
        verify_model_file(&file, &model_dir.join(file.relative_path))?;
    }
    Ok(())
}

pub fn verify_model_file(file: &ModelFile, path: &Path) -> Result<(), String> {
    let input = std::fs::File::open(path)
        .map_err(|error| format!("Failed to open {}: {error}", path.display()))?;
    let mut input = std::io::BufReader::new(input);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != file.sha256 {
        return Err(format!(
            "Checksum mismatch for {}: expected {}, got {}",
            path.display(),
            file.sha256,
            actual
        ));
    }
    Ok(())
}

pub fn mark_model_verified(model_dir: &Path) -> Result<(), String> {
    std::fs::write(model_dir.join(REVISION_MARKER), MODEL_REVISION)
        .map_err(|error| format!("Failed to write SenseVoice revision marker: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_model_size_matches_files() {
        assert_eq!(
            MODEL_FILES.iter().map(|file| file.size).sum::<u64>(),
            MODEL_SIZE_BYTES
        );
    }
}
