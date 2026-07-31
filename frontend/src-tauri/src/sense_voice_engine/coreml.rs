use coreml_native::{BorrowedTensor, ComputeUnits, Model};
use std::fs;
use std::path::Path;
use std::sync::Mutex;

const PREPROCESSOR_DIR: &str = "SenseVoicePreprocessor.mlmodelc";
const ENCODER_DIR: &str = "SenseVoiceSmall_int8.mlmodelc";
const VOCAB_FILE: &str = "vocab.json";
const FEATURE_DIM: usize = 560;
const FEATURE_BUCKETS: [usize; 5] = [128, 256, 512, 1024, 1800];
const QUERY_TOKENS: usize = 4;
const BLANK_ID: usize = 0;
const LANGUAGE_AUTO: i32 = 0;
const WITH_ITN: i32 = 14;
const WAVEFORM_SCALE: f32 = 32_768.0;
const MAX_AUDIO_SAMPLES: usize = 30 * 16_000;
const MIN_AUDIO_SAMPLES: usize = 3_200;

#[derive(thiserror::Error, Debug)]
pub enum CoreMlSenseVoiceError {
    #[error("CoreML model initialization failed: {0}")]
    Initialization(String),
    #[error("CoreML inference failed: {0}")]
    Inference(String),
}

pub struct CoreMlSenseVoiceModel {
    pipeline: Mutex<CoreMlSenseVoicePipeline>,
}

impl CoreMlSenseVoiceModel {
    pub fn new(model_dir: &Path) -> Result<Self, CoreMlSenseVoiceError> {
        let pipeline = CoreMlSenseVoicePipeline::load(model_dir)?;
        Ok(Self {
            pipeline: Mutex::new(pipeline),
        })
    }

    pub fn transcribe_samples(&self, samples: &[f32]) -> Result<String, CoreMlSenseVoiceError> {
        if samples.len() < MIN_AUDIO_SAMPLES {
            return Ok(String::new());
        }
        let pipeline = self.pipeline.lock().map_err(|_| {
            CoreMlSenseVoiceError::Inference("CoreML pipeline lock was poisoned".to_string())
        })?;
        let mut parts = Vec::new();
        let mut offset = 0;
        while offset < samples.len() {
            let remaining = samples.len() - offset;
            let chunk_len = if remaining <= MAX_AUDIO_SAMPLES {
                remaining
            } else if remaining - MAX_AUDIO_SAMPLES < MIN_AUDIO_SAMPLES {
                remaining - MIN_AUDIO_SAMPLES
            } else {
                MAX_AUDIO_SAMPLES
            };
            let chunk = &samples[offset..offset + chunk_len];
            let text = pipeline.transcribe_chunk(chunk)?;
            if !text.is_empty() {
                parts.push(text);
            }
            offset += chunk_len;
        }
        Ok(join_chunk_text(&parts))
    }
}

struct CoreMlSenseVoicePipeline {
    preprocessor: Model,
    encoder: Model,
    vocabulary: Vec<String>,
}

impl CoreMlSenseVoicePipeline {
    fn load(model_dir: &Path) -> Result<Self, CoreMlSenseVoiceError> {
        let preprocessor = Model::load(model_dir.join(PREPROCESSOR_DIR), ComputeUnits::CpuOnly)
            .map_err(|error| CoreMlSenseVoiceError::Initialization(error.to_string()))?;
        let encoder = Model::load(
            model_dir.join(ENCODER_DIR),
            ComputeUnits::CpuAndNeuralEngine,
        )
        .map_err(|error| CoreMlSenseVoiceError::Initialization(error.to_string()))?;
        let vocabulary = fs::read_to_string(model_dir.join(VOCAB_FILE))
            .map_err(|error| CoreMlSenseVoiceError::Initialization(error.to_string()))
            .and_then(|content| {
                serde_json::from_str::<Vec<String>>(&content)
                    .map_err(|error| CoreMlSenseVoiceError::Initialization(error.to_string()))
            })?;
        if vocabulary.len() != 25_055 {
            return Err(CoreMlSenseVoiceError::Initialization(format!(
                "Expected 25055 vocabulary tokens, found {}",
                vocabulary.len()
            )));
        }

        log::info!(
            "Loaded SenseVoice CoreML pipeline (preprocessor='CPU only', encoder='CPU + Neural Engine')"
        );
        Ok(Self {
            preprocessor,
            encoder,
            vocabulary,
        })
    }

    fn transcribe_chunk(&self, samples: &[f32]) -> Result<String, CoreMlSenseVoiceError> {
        let scaled_waveform: Vec<f32> = samples
            .iter()
            .map(|sample| sample * WAVEFORM_SCALE)
            .collect();
        let waveform = BorrowedTensor::from_f32(&scaled_waveform, &[1, samples.len()])
            .map_err(|error| CoreMlSenseVoiceError::Inference(error.to_string()))?;
        let preprocessor_outputs = self
            .preprocessor
            .predict(&[("waveform", &waveform)])
            .map_err(|error| CoreMlSenseVoiceError::Inference(error.to_string()))?;
        let (features, feature_shape) = preprocessor_outputs
            .get_f32("features")
            .map_err(|error| CoreMlSenseVoiceError::Inference(error.to_string()))?;
        if feature_shape.len() != 3 || feature_shape[0] != 1 || feature_shape[2] != FEATURE_DIM {
            return Err(CoreMlSenseVoiceError::Inference(format!(
                "Unexpected SenseVoice feature shape: {feature_shape:?}"
            )));
        }

        let valid_frames = feature_shape[1].min(*FEATURE_BUCKETS.last().unwrap_or(&1800));
        let bucket = FEATURE_BUCKETS
            .iter()
            .copied()
            .find(|bucket| *bucket >= valid_frames)
            .unwrap_or(*FEATURE_BUCKETS.last().unwrap_or(&1800));
        let mut padded_features = vec![0.0_f32; bucket * FEATURE_DIM];
        let feature_values = valid_frames * FEATURE_DIM;
        padded_features[..feature_values].copy_from_slice(&features[..feature_values]);

        let speech = BorrowedTensor::from_f32(&padded_features, &[1, bucket, FEATURE_DIM])
            .map_err(|error| CoreMlSenseVoiceError::Inference(error.to_string()))?;
        let speech_lengths_values = [valid_frames as i32];
        let language_values = [LANGUAGE_AUTO];
        let text_norm_values = [WITH_ITN];
        let speech_lengths = BorrowedTensor::from_i32(&speech_lengths_values, &[1])
            .map_err(|error| CoreMlSenseVoiceError::Inference(error.to_string()))?;
        let language = BorrowedTensor::from_i32(&language_values, &[1])
            .map_err(|error| CoreMlSenseVoiceError::Inference(error.to_string()))?;
        let text_norm = BorrowedTensor::from_i32(&text_norm_values, &[1])
            .map_err(|error| CoreMlSenseVoiceError::Inference(error.to_string()))?;
        let encoder_outputs = self
            .encoder
            .predict(&[
                ("speech", &speech),
                ("speech_lengths", &speech_lengths),
                ("language", &language),
                ("textnorm", &text_norm),
            ])
            .map_err(|error| CoreMlSenseVoiceError::Inference(error.to_string()))?;
        let (logits, logits_shape) = encoder_outputs
            .get_f32("ctc_logits")
            .map_err(|error| CoreMlSenseVoiceError::Inference(error.to_string()))?;

        decode_logits(
            &logits,
            &logits_shape,
            QUERY_TOKENS + valid_frames,
            &self.vocabulary,
        )
        .map_err(CoreMlSenseVoiceError::Inference)
    }
}

fn decode_logits(
    logits: &[f32],
    shape: &[usize],
    valid_frames: usize,
    vocabulary: &[String],
) -> Result<String, String> {
    if shape.len() != 3 || shape[0] != 1 || shape[2] != vocabulary.len() {
        return Err(format!("Unexpected SenseVoice logits shape: {shape:?}"));
    }
    let time_steps = shape[1];
    let vocab_size = shape[2];
    if logits.len() != time_steps * vocab_size {
        return Err(format!(
            "SenseVoice logits contain {} values for {}x{}",
            logits.len(),
            time_steps,
            vocab_size
        ));
    }

    let mut previous = None;
    let mut token_ids = Vec::new();
    for frame in logits
        .chunks_exact(vocab_size)
        .take(valid_frames.min(time_steps))
    {
        let token_id = frame
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| {
                left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(index, _)| index)
            .unwrap_or(BLANK_ID);
        if token_id != BLANK_ID && previous != Some(token_id) {
            token_ids.push(token_id);
        }
        previous = Some(token_id);
    }

    let text = token_ids
        .into_iter()
        .filter_map(|token_id| vocabulary.get(token_id))
        .cloned()
        .collect::<String>()
        .replace('\u{2581}', " ");
    Ok(normalize_text(&strip_metadata_tags(&text)))
}

fn strip_metadata_tags(text: &str) -> String {
    let mut remaining = text;
    let mut output = String::with_capacity(text.len());
    while let Some(start) = remaining.find("<|") {
        output.push_str(&remaining[..start]);
        let tag = &remaining[start + 2..];
        let Some(end) = tag.find("|>") else {
            output.push_str(&remaining[start..]);
            return output;
        };
        remaining = &tag[end + 2..];
    }
    output.push_str(remaining);
    output
}

fn join_chunk_text(parts: &[String]) -> String {
    normalize_text(&parts.join(" "))
}

fn normalize_text(text: &str) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let characters: Vec<char> = compact.chars().collect();
    let mut output = String::with_capacity(compact.len());
    for (index, character) in characters.iter().copied().enumerate() {
        if character == ' ' {
            let previous = output.chars().next_back();
            let next = characters.get(index + 1).copied();
            if previous.is_some_and(is_cjk) && next.is_some_and(is_cjk) {
                continue;
            }
        }
        output.push(character);
    }
    output.trim().to_string()
}

fn is_cjk(character: char) -> bool {
    matches!(
        character as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctc_decode_collapses_repeats_and_strips_metadata() {
        let mut vocabulary = vec![String::new(); 4];
        vocabulary[1] = "<|zh|>".to_string();
        vocabulary[2] = "\u{2581}\u{4F60}".to_string();
        vocabulary[3] = "\u{2581}\u{597D}".to_string();
        let logits = vec![
            0.0, 5.0, 0.0, 0.0, // language tag
            0.0, 0.0, 5.0, 0.0, // token 2
            0.0, 0.0, 5.0, 0.0, // repeated token 2
            5.0, 0.0, 0.0, 0.0, // blank
            0.0, 0.0, 0.0, 5.0, // token 3
        ];

        assert_eq!(
            decode_logits(&logits, &[1, 5, 4], 5, &vocabulary).unwrap(),
            "\u{4F60}\u{597D}"
        );
    }
}
