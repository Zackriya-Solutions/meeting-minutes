use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const SENSE_VOICE_MODEL: &str = "sense-voice-small-int8";
pub const SENSE_VOICE_FP16_MODEL: &str = "sense-voice-small-fp16";
pub const SENSE_VOICE_FP32_MODEL: &str = "sense-voice-small-fp32";
pub const REVISION_MARKER: &str = ".meetily-model-revision";

#[derive(Debug, Clone, Copy)]
pub struct ModelFile {
    pub relative_path: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct ModelDefinition {
    pub name: &'static str,
    pub revision: &'static str,
    pub base_url: &'static str,
    pub encoder_dir: &'static str,
    pub description: &'static str,
}

const COREML_REVISION: &str = "cdea3526163035c19915d4a10268992d018ebd46";
const COREML_BASE_URL: &str =
    "https://huggingface.co/FluidInference/sensevoice-small-coreml/resolve";
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
const ONNX_REVISION: &str = "2365baeacb507f821a0c8120fcee3d484dba7a07";
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
const ONNX_BASE_URL: &str =
    "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve";

const PREPROCESSOR_FILES: [ModelFile; 4] = [
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
];
const VOCAB_FILE: ModelFile = ModelFile {
    relative_path: "vocab.json",
    size: 352_064,
    sha256: "a2594fc1474e78973149cba8cd1f603ebed8c39c7decb470631f66e70ce58e97",
};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const INT8_FILES: [ModelFile; 4] = [
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
];
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const FP16_FILES: [ModelFile; 4] = [
    ModelFile {
        relative_path: "SenseVoiceSmall.mlmodelc/analytics/coremldata.bin",
        size: 243,
        sha256: "2dd2919d1ef534ecd4d0c9843dea078b0ad337e0918e692d9811cb16a31fb02b",
    },
    ModelFile {
        relative_path: "SenseVoiceSmall.mlmodelc/coremldata.bin",
        size: 436,
        sha256: "8af6326236369150e5540e15996877a71b281e98cb9ede6b646c2f4b3d9be88c",
    },
    ModelFile {
        relative_path: "SenseVoiceSmall.mlmodelc/model.mil",
        size: 1_003_095,
        sha256: "c53547bea5b26f36f603a0ef4bda5b47b72a409bf9fd9eafae1d21cfbd51aedf",
    },
    ModelFile {
        relative_path: "SenseVoiceSmall.mlmodelc/weights/weight.bin",
        size: 468_060_094,
        sha256: "f435f29513464bcda175e449fd72e28ef5183b963f116394a38eadbbc12ca694",
    },
];
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const FP32_FILES: [ModelFile; 4] = [
    ModelFile {
        relative_path: "SenseVoiceSmall_fp32.mlmodelc/analytics/coremldata.bin",
        size: 243,
        sha256: "09bdfe5eee1fd3cc70fc39e1e144ede5118e138c3c2dd52a2822d0d72fbb91f8",
    },
    ModelFile {
        relative_path: "SenseVoiceSmall_fp32.mlmodelc/coremldata.bin",
        size: 396,
        sha256: "ba5a1b5d9bf9b1b85ef2d1f69717e1f4424cc72e7316fc3edb0b604e449f9919",
    },
    ModelFile {
        relative_path: "SenseVoiceSmall_fp32.mlmodelc/model.mil",
        size: 915_059,
        sha256: "4569b5ac67d69a50b993c1d3918e6d569f2d22b3a129653cc4e6c8f0c270cc9e",
    },
    ModelFile {
        relative_path: "SenseVoiceSmall_fp32.mlmodelc/weights/weight.bin",
        size: 940_100_992,
        sha256: "62919f3a37419a1e4ede3763d6efcf2ae9ed320e6bd9fb4a37d2b15ef891b92d",
    },
];
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
const ONNX_FILES: [ModelFile; 3] = [
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

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub const MODEL_DEFINITIONS: [ModelDefinition; 3] = [
    ModelDefinition { name: SENSE_VOICE_MODEL, revision: COREML_REVISION, base_url: COREML_BASE_URL, encoder_dir: "SenseVoiceSmall_int8.mlmodelc", description: "Fast multilingual recognition with INT8 weights, accelerated by Apple Neural Engine" },
    ModelDefinition { name: SENSE_VOICE_FP16_MODEL, revision: COREML_REVISION, base_url: COREML_BASE_URL, encoder_dir: "SenseVoiceSmall.mlmodelc", description: "Balanced multilingual recognition with FP16 weights, accelerated by Apple Neural Engine" },
    ModelDefinition { name: SENSE_VOICE_FP32_MODEL, revision: COREML_REVISION, base_url: COREML_BASE_URL, encoder_dir: "SenseVoiceSmall_fp32.mlmodelc", description: "Highest-fidelity multilingual recognition with FP32 weights; uses substantially more disk and memory" },
];
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
pub const MODEL_DEFINITIONS: [ModelDefinition; 1] = [ModelDefinition {
    name: SENSE_VOICE_MODEL,
    revision: ONNX_REVISION,
    base_url: ONNX_BASE_URL,
    encoder_dir: "",
    description: "Fast multilingual recognition with INT8 ONNX weights",
}];

pub fn model_definition(name: &str) -> Option<&'static ModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.name == name)
}
pub fn model_files(name: &str) -> Option<Vec<ModelFile>> {
    let mut files = Vec::new();
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        files.extend(PREPROCESSOR_FILES);
        files.extend(match name {
            SENSE_VOICE_MODEL => INT8_FILES,
            SENSE_VOICE_FP16_MODEL => FP16_FILES,
            SENSE_VOICE_FP32_MODEL => FP32_FILES,
            _ => return None,
        });
        files.push(VOCAB_FILE);
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        if name != SENSE_VOICE_MODEL {
            return None;
        }
        files.extend(ONNX_FILES);
    }
    Some(files)
}
pub fn model_size(name: &str) -> Option<u64> {
    model_files(name).map(|files| files.iter().map(|file| file.size).sum())
}

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

pub fn model_info(models_dir: &Path, definition: &ModelDefinition) -> ModelInfo {
    let model_dir = models_dir.join(definition.name);
    ModelInfo {
        name: definition.name.to_string(),
        path: model_dir.clone(),
        size_mb: model_size(definition.name).unwrap_or_default() / 1_048_576,
        status: inspect_model(definition.name, &model_dir),
        description: definition.description.to_string(),
    }
}
pub fn inspect_model(name: &str, model_dir: &Path) -> ModelStatus {
    let Some(files) = model_files(name) else {
        return ModelStatus::Error(format!("Unknown SenseVoice model: {name}"));
    };
    let expected_size = files.iter().map(|file| file.size).sum();
    let mut present_bytes = 0;
    let mut present_files = 0;
    for file in &files {
        match std::fs::metadata(model_dir.join(file.relative_path)) {
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
    if present_files == files.len() && present_bytes == expected_size {
        match std::fs::read_to_string(model_dir.join(REVISION_MARKER)) {
            Ok(revision) if revision.trim() == model_definition(name).unwrap().revision => {
                ModelStatus::Available
            }
            _ => ModelStatus::Corrupted {
                file_size: present_bytes,
                expected_size,
            },
        }
    } else if present_files == 0 {
        ModelStatus::Missing
    } else {
        ModelStatus::Corrupted {
            file_size: present_bytes,
            expected_size,
        }
    }
}
pub fn verify_model_hashes(name: &str, model_dir: &Path) -> Result<(), String> {
    for file in model_files(name).ok_or_else(|| format!("Unknown SenseVoice model: {name}"))? {
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
pub fn mark_model_verified(name: &str, model_dir: &Path) -> Result<(), String> {
    let revision = model_definition(name)
        .ok_or_else(|| format!("Unknown SenseVoice model: {name}"))?
        .revision;
    std::fs::write(model_dir.join(REVISION_MARKER), revision)
        .map_err(|error| format!("Failed to write SenseVoice revision marker: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_model_sizes_match_files() {
        for definition in MODEL_DEFINITIONS {
            assert_eq!(
                model_files(definition.name)
                    .unwrap()
                    .iter()
                    .map(|file| file.size)
                    .sum::<u64>(),
                model_size(definition.name).unwrap()
            );
        }
    }
}
