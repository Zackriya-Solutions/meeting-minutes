use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const SENSE_VOICE_MODEL: &str = "sense-voice-small-int8";
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub const MODEL_REVISION: &str = "cdea3526163035c19915d4a10268992d018ebd46";
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
pub const MODEL_REVISION: &str = "2365baeacb507f821a0c8120fcee3d484dba7a07";
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub const MODEL_BASE_URL: &str =
    "https://huggingface.co/FluidInference/sensevoice-small-coreml/resolve";
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
pub const MODEL_BASE_URL: &str =
    "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve";
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub const MODEL_SIZE_BYTES: u64 = 239_913_642;
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
pub const MODEL_SIZE_BYTES: u64 = 239_549_806;
pub const REVISION_MARKER: &str = ".meetily-model-revision";

#[derive(Debug, Clone, Copy)]
pub struct ModelFile {
    pub relative_path: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub const MODEL_FILES: [ModelFile; 9] = [
    ModelFile {
        relative_path: "SenseVoicePreprocessor.mlmodelc/analytics/coremldata.bin",
        size: 243,
        sha256: "5bdb0b132e48c7e852ec18eeba7e217b6cb7153e6a939ce76b5ed17242e956dd",
    },
    ModelFile {
        relative_path: "SenseVoicePreprocessor.mlmodelc/coremldata.bin",
        size: 330,
        sha256: "e64cc73b2a9b01bad799a23874bc20dba3cf3342c23e3f60012c3e884f682944",
    },
    ModelFile {
        relative_path: "SenseVoicePreprocessor.mlmodelc/model.mil",
        size: 15_008,
        sha256: "1b9b18be0a35b11165269b1ca071a30af736deb314d8bd82d9540c769137a70e",
    },
    ModelFile {
        relative_path: "SenseVoicePreprocessor.mlmodelc/weights/weight.bin",
        size: 3_037_504,
        sha256: "69c630a115da5e4db36ec41662f0b776c0ef33ec6776d86f8cdaaba022518396",
    },
    ModelFile {
        relative_path: "SenseVoiceSmall_int8.mlmodelc/analytics/coremldata.bin",
        size: 243,
        sha256: "ab5e9ee0d49e1f88838f1c2178cbe58a20dac12b50c4da803a75a54c6229845a",
    },
    ModelFile {
        relative_path: "SenseVoiceSmall_int8.mlmodelc/coremldata.bin",
        size: 436,
        sha256: "55ef1c194e641418817d7d07f6bfbd8032571e800b81264caba37eb63a95335b",
    },
    ModelFile {
        relative_path: "SenseVoiceSmall_int8.mlmodelc/model.mil",
        size: 1_134_696,
        sha256: "015fe7242a15eeb2fc0ca7f908ca3a09a5826b36e7d7f704803c8bbe60c1a148",
    },
    ModelFile {
        relative_path: "SenseVoiceSmall_int8.mlmodelc/weights/weight.bin",
        size: 235_373_118,
        sha256: "dab122c65d5043cba5b47561d5c1d3a049dd123c662e802d9dbce8fdd0505a38",
    },
    ModelFile {
        relative_path: "vocab.json",
        size: 352_064,
        sha256: "a2594fc1474e78973149cba8cd1f603ebed8c39c7decb470631f66e70ce58e97",
    },
];

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
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
        description: if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            "Fast multilingual recognition accelerated by Apple Neural Engine".to_string()
        } else {
            "Fast Mandarin, Cantonese, English, Japanese, and Korean recognition".to_string()
        },
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
