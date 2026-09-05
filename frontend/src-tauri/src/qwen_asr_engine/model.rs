use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const QWEN3_ASR_MODEL: &str = "qwen3-asr-0.6b-int8";
pub const QWEN3_ASR_REVISION: &str = "68818b2313fe77bd06f6a7c5068ff3ef59d02b8a";
pub const QWEN3_ASR_SIZE_BYTES: u64 = 987_015_347;
const REVISION_MARKER: &str = ".meetily-model-revision";

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

#[derive(Debug, Clone, Copy)]
pub struct ModelFile {
    pub relative_path: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

pub const MODEL_FILES: [ModelFile; 6] = [
    ModelFile {
        relative_path: "conv_frontend.onnx",
        size: 44_148_281,
        sha256: "d22dc4423e0940e49884e903d2ea2f7e5567c14fc1aed97e4e26d6b8f208ef9e",
    },
    ModelFile {
        relative_path: "encoder.int8.onnx",
        size: 182_491_662,
        sha256: "60748d3e6744a57c9c91e1b17424a6c2990567e8adceb0783940c03ed98fa9d9",
    },
    ModelFile {
        relative_path: "decoder.int8.onnx",
        size: 755_914_231,
        sha256: "4f6885be5959ae26af3089d38ee7972c5fafbeeb1cf8d5e76eab6d8b61ca5771",
    },
    ModelFile {
        relative_path: "tokenizer/merges.txt",
        size: 1_671_853,
        sha256: "8831e4f1a044471340f7c0a83d7bd71306a5b867e95fd870f74d0c5308a904d5",
    },
    ModelFile {
        relative_path: "tokenizer/tokenizer_config.json",
        size: 12_487,
        sha256: "4942d005604266809309cabc9f4e9cb89ce855d59b14681fdc0e1cc62ea26c4c",
    },
    ModelFile {
        relative_path: "tokenizer/vocab.json",
        size: 2_776_833,
        sha256: "ca10d7e9fb3ed18575dd1e277a2579c16d108e32f27439684afa0e10b1440910",
    },
];

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

    if present_files == MODEL_FILES.len() && present_bytes == QWEN3_ASR_SIZE_BYTES {
        match std::fs::read_to_string(model_dir.join(REVISION_MARKER)) {
            Ok(revision) if revision.trim() == QWEN3_ASR_REVISION => ModelStatus::Available,
            _ => ModelStatus::Corrupted {
                file_size: present_bytes,
                expected_size: QWEN3_ASR_SIZE_BYTES,
            },
        }
    } else if present_files == 0 {
        ModelStatus::Missing
    } else {
        ModelStatus::Corrupted {
            file_size: present_bytes,
            expected_size: QWEN3_ASR_SIZE_BYTES,
        }
    }
}

pub fn verify_model_hashes(model_dir: &Path) -> Result<(), String> {
    for file in MODEL_FILES {
        let path = model_dir.join(file.relative_path);
        let input = std::fs::File::open(&path)
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
                "SHA-256 mismatch for {}: expected {}, got {}",
                file.relative_path, file.sha256, actual
            ));
        }
    }
    Ok(())
}

pub fn mark_model_verified(model_dir: &Path) -> Result<(), String> {
    std::fs::write(
        model_dir.join(REVISION_MARKER),
        format!("{}\n", QWEN3_ASR_REVISION),
    )
    .map_err(|error| format!("Failed to write Qwen3-ASR revision marker: {error}"))
}

pub fn model_info(models_dir: &Path) -> ModelInfo {
    let path = models_dir.join(QWEN3_ASR_MODEL);
    ModelInfo {
        name: QWEN3_ASR_MODEL.to_string(),
        status: inspect_model(&path),
        path,
        size_mb: (QWEN3_ASR_SIZE_BYTES + 1_048_575) / 1_048_576,
        description: "Multilingual Qwen3-ASR with Mandarin dialect and code-switching support"
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_size_matches_files() {
        assert_eq!(
            MODEL_FILES.iter().map(|file| file.size).sum::<u64>(),
            QWEN3_ASR_SIZE_BYTES
        );
    }

    #[test]
    fn empty_directory_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(inspect_model(temp.path()), ModelStatus::Missing);
    }
}
