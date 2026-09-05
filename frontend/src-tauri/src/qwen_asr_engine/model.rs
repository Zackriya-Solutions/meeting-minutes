use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const QWEN3_ASR_MODEL: &str = "qwen3-asr-0.6b-int8";
pub const QWEN3_ASR_1_7B_MODEL: &str = "qwen3-asr-1.7b-int8";
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

#[derive(Debug, Clone, Copy)]
pub struct ModelSpec {
    pub name: &'static str,
    pub revision: &'static str,
    pub base_url: &'static str,
    pub size_bytes: u64,
    pub description: &'static str,
    pub files: &'static [ModelFile],
}

const QWEN3_ASR_0_6B_FILES: [ModelFile; 6] = [
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

const QWEN3_ASR_1_7B_FILES: [ModelFile; 6] = [
    ModelFile {
        relative_path: "conv_frontend.onnx",
        size: 48_080_441,
        sha256: "fa894a4ba53da6a4238f2a6ca0b09362e505d39cecbd646051b033e2e8d7e2fb",
    },
    ModelFile {
        relative_path: "encoder.int8.onnx",
        size: 314_222_162,
        sha256: "436fbd910a0c8914851e5ac1354e807be9f283d08a5da728adaa609731c41469",
    },
    ModelFile {
        relative_path: "decoder.int8.onnx",
        size: 2_037_458_645,
        sha256: "c43c853fa6e97d08365cb8a5502b360b595cd43c00dc60e4d8ca7cc18cad460b",
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

const MODEL_SPECS: [ModelSpec; 2] = [
    ModelSpec {
        name: QWEN3_ASR_MODEL,
        revision: "68818b2313fe77bd06f6a7c5068ff3ef59d02b8a",
        base_url:
            "https://huggingface.co/csukuangfj2/sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25/resolve",
        size_bytes: 987_015_347,
        description: "Compact multilingual model with Chinese dialect and code-switching support",
        files: &QWEN3_ASR_0_6B_FILES,
    },
    ModelSpec {
        name: QWEN3_ASR_1_7B_MODEL,
        revision: "66fb5ea2d4d1682ff8a663bf7e788913604996a0",
        base_url: "https://huggingface.co/ilmina/qwen3-asr-1.7b-sherpa-onnx/resolve",
        size_bytes: 2_404_222_421,
        description: "Higher-capacity multilingual model for improved recognition accuracy",
        files: &QWEN3_ASR_1_7B_FILES,
    },
];

pub fn model_specs() -> &'static [ModelSpec] {
    &MODEL_SPECS
}

pub fn model_spec(model_name: &str) -> Option<&'static ModelSpec> {
    MODEL_SPECS.iter().find(|spec| spec.name == model_name)
}

pub fn is_supported_model(model_name: &str) -> bool {
    model_spec(model_name).is_some()
}

pub fn inspect_model(spec: &ModelSpec, model_dir: &Path) -> ModelStatus {
    let mut present_bytes = 0;
    let mut present_files = 0;

    for file in spec.files {
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

    if present_files == spec.files.len() && present_bytes == spec.size_bytes {
        match std::fs::read_to_string(model_dir.join(REVISION_MARKER)) {
            Ok(revision) if revision.trim() == spec.revision => ModelStatus::Available,
            _ => ModelStatus::Corrupted {
                file_size: present_bytes,
                expected_size: spec.size_bytes,
            },
        }
    } else if present_files == 0 {
        ModelStatus::Missing
    } else {
        ModelStatus::Corrupted {
            file_size: present_bytes,
            expected_size: spec.size_bytes,
        }
    }
}

pub fn verify_model_hashes(spec: &ModelSpec, model_dir: &Path) -> Result<(), String> {
    for file in spec.files {
        let path = model_dir.join(file.relative_path);
        verify_model_file(file, &path)?;
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
            "SHA-256 mismatch for {}: expected {}, got {}",
            file.relative_path, file.sha256, actual
        ));
    }
    Ok(())
}

pub fn mark_model_verified(spec: &ModelSpec, model_dir: &Path) -> Result<(), String> {
    std::fs::write(
        model_dir.join(REVISION_MARKER),
        format!("{}\n", spec.revision),
    )
    .map_err(|error| format!("Failed to write Qwen3-ASR revision marker: {error}"))
}

pub fn model_info(models_dir: &Path, spec: &ModelSpec) -> ModelInfo {
    let path = models_dir.join(spec.name);
    ModelInfo {
        name: spec.name.to_string(),
        status: inspect_model(spec, &path),
        path,
        size_mb: (spec.size_bytes + 1_048_575) / 1_048_576,
        description: spec.description.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_sizes_match_model_files() {
        for spec in model_specs() {
            assert_eq!(
                spec.files.iter().map(|file| file.size).sum::<u64>(),
                spec.size_bytes,
                "{} has inconsistent model metadata",
                spec.name
            );
        }
    }

    #[test]
    fn empty_directory_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        for spec in model_specs() {
            assert_eq!(inspect_model(spec, temp.path()), ModelStatus::Missing);
        }
    }

    #[test]
    fn model_lookup_rejects_unknown_names() {
        assert!(model_spec(QWEN3_ASR_MODEL).is_some());
        assert!(model_spec(QWEN3_ASR_1_7B_MODEL).is_some());
        assert!(model_spec("qwen3-asr-unknown").is_none());
    }

    #[test]
    fn file_verification_detects_same_size_corruption() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("model.onnx");
        let file = ModelFile {
            relative_path: "model.onnx",
            size: 5,
            sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        };

        std::fs::write(&path, b"hello").unwrap();
        assert!(verify_model_file(&file, &path).is_ok());

        std::fs::write(&path, b"jello").unwrap();
        assert!(verify_model_file(&file, &path).is_err());
    }
}
