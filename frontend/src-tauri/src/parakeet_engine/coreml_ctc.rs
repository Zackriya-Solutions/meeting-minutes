use super::ctc::{
    decode_ctc_logits, join_chunk_text, PARAKEET_CTC_ZH_CN_BLANK_ID, PARAKEET_CTC_ZH_CN_MAX_SAMPLES,
};
use super::model::TimestampedResult;
use coreml_native::{BorrowedTensor, ComputeUnits, Model};
use std::fs;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const PREPROCESSOR_DIR: &str = "Preprocessor.mlmodelc";
const ENCODER_DIR: &str = "Encoder-v2-int8.mlmodelc";
const DECODER_DIR: &str = "Decoder.mlmodelc";
const VOCAB_FILE: &str = "vocab.json";

#[derive(thiserror::Error, Debug)]
pub enum CoreMlCtcError {
    #[error("CoreML model initialization failed: {0}")]
    Initialization(String),
    #[error("CoreML inference failed: {0}")]
    Inference(String),
    #[error("CoreML worker is not available")]
    WorkerUnavailable,
}

enum WorkerRequest {
    Transcribe {
        samples: Vec<f32>,
        response: mpsc::Sender<Result<TimestampedResult, String>>,
    },
    Shutdown,
}

pub struct CoreMlCtcModel {
    request_sender: SyncSender<WorkerRequest>,
    worker: Option<JoinHandle<()>>,
}

impl CoreMlCtcModel {
    pub fn new(model_dir: &Path) -> Result<Self, CoreMlCtcError> {
        let (request_sender, request_receiver) = mpsc::sync_channel(1);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let model_dir = model_dir.to_path_buf();

        let worker = thread::Builder::new()
            .name("parakeet-coreml-ctc".to_string())
            .spawn(move || {
                let pipeline = CoreMlCtcPipeline::load(&model_dir);
                let ready = pipeline.as_ref().map(|_| ()).map_err(ToString::to_string);
                let _ = ready_sender.send(ready);

                let Ok(mut pipeline) = pipeline else {
                    return;
                };
                pipeline.run(request_receiver);
            })
            .map_err(|error| CoreMlCtcError::Initialization(error.to_string()))?;

        match ready_receiver.recv_timeout(Duration::from_secs(120)) {
            Ok(Ok(())) => Ok(Self {
                request_sender,
                worker: Some(worker),
            }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(CoreMlCtcError::Initialization(error))
            }
            Err(error) => {
                let _ = request_sender.try_send(WorkerRequest::Shutdown);
                let _ = worker.join();
                Err(CoreMlCtcError::Initialization(format!(
                    "Timed out while loading CoreML model: {error}"
                )))
            }
        }
    }

    pub fn transcribe_samples(
        &self,
        samples: Vec<f32>,
    ) -> Result<TimestampedResult, CoreMlCtcError> {
        if samples.is_empty() {
            return Err(CoreMlCtcError::Inference(
                "Audio contains no samples".to_string(),
            ));
        }
        if samples.iter().all(|sample| sample.abs() < 1.0e-6) {
            return Ok(TimestampedResult {
                text: String::new(),
                timestamps: Vec::new(),
                tokens: Vec::new(),
            });
        }

        let mut texts = Vec::new();
        let mut timestamps = Vec::new();
        let mut tokens = Vec::new();

        for (chunk_index, chunk) in samples.chunks(PARAKEET_CTC_ZH_CN_MAX_SAMPLES).enumerate() {
            let mut result = self.transcribe_chunk(chunk.to_vec())?;
            let timestamp_offset =
                chunk_index as f32 * PARAKEET_CTC_ZH_CN_MAX_SAMPLES as f32 / 16_000.0;
            timestamps.extend(
                result
                    .timestamps
                    .drain(..)
                    .map(|value| value + timestamp_offset),
            );
            tokens.append(&mut result.tokens);
            if !result.text.is_empty() {
                texts.push(result.text);
            }
        }

        Ok(TimestampedResult {
            text: join_chunk_text(&texts),
            timestamps,
            tokens,
        })
    }

    fn transcribe_chunk(&self, samples: Vec<f32>) -> Result<TimestampedResult, CoreMlCtcError> {
        let (response_sender, response_receiver) = mpsc::channel();
        self.request_sender
            .send(WorkerRequest::Transcribe {
                samples,
                response: response_sender,
            })
            .map_err(|_| CoreMlCtcError::WorkerUnavailable)?;

        response_receiver
            .recv()
            .map_err(|_| CoreMlCtcError::WorkerUnavailable)?
            .map_err(CoreMlCtcError::Inference)
    }
}

impl Drop for CoreMlCtcModel {
    fn drop(&mut self) {
        let _ = self.request_sender.try_send(WorkerRequest::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct CoreMlCtcPipeline {
    preprocessor: Model,
    encoder: Model,
    decoder: Model,
    vocab: Vec<String>,
}

impl CoreMlCtcPipeline {
    fn load(model_dir: &Path) -> Result<Self, CoreMlCtcError> {
        let preprocessor = Model::load(model_dir.join(PREPROCESSOR_DIR), ComputeUnits::All)
            .map_err(|error| CoreMlCtcError::Initialization(error.to_string()))?;
        let encoder = Model::load(model_dir.join(ENCODER_DIR), ComputeUnits::All)
            .map_err(|error| CoreMlCtcError::Initialization(error.to_string()))?;
        let decoder = Model::load(model_dir.join(DECODER_DIR), ComputeUnits::All)
            .map_err(|error| CoreMlCtcError::Initialization(error.to_string()))?;

        let vocab = fs::read_to_string(model_dir.join(VOCAB_FILE))
            .map_err(|error| CoreMlCtcError::Initialization(error.to_string()))
            .and_then(|content| {
                serde_json::from_str::<Vec<String>>(&content)
                    .map_err(|error| CoreMlCtcError::Initialization(error.to_string()))
            })?;
        if vocab.len() != PARAKEET_CTC_ZH_CN_BLANK_ID {
            return Err(CoreMlCtcError::Initialization(format!(
                "Expected {} vocabulary tokens, found {}",
                PARAKEET_CTC_ZH_CN_BLANK_ID,
                vocab.len()
            )));
        }

        Ok(Self {
            preprocessor,
            encoder,
            decoder,
            vocab,
        })
    }

    fn run(&mut self, receiver: Receiver<WorkerRequest>) {
        while let Ok(request) = receiver.recv() {
            match request {
                WorkerRequest::Transcribe { samples, response } => {
                    let result = self
                        .transcribe_chunk(&samples)
                        .map_err(|error| error.to_string());
                    let _ = response.send(result);
                }
                WorkerRequest::Shutdown => break,
            }
        }
    }

    fn transcribe_chunk(&self, samples: &[f32]) -> Result<TimestampedResult, CoreMlCtcError> {
        if samples.len() > PARAKEET_CTC_ZH_CN_MAX_SAMPLES {
            return Err(CoreMlCtcError::Inference(format!(
                "CoreML CTC input exceeds {} samples",
                PARAKEET_CTC_ZH_CN_MAX_SAMPLES
            )));
        }

        let mut padded_audio = vec![0.0_f32; PARAKEET_CTC_ZH_CN_MAX_SAMPLES];
        padded_audio[..samples.len()].copy_from_slice(samples);

        let audio_length = [samples.len() as i32];
        let audio = BorrowedTensor::from_f32(&padded_audio, &[1, PARAKEET_CTC_ZH_CN_MAX_SAMPLES])
            .map_err(|error| CoreMlCtcError::Inference(error.to_string()))?;
        let audio_length = BorrowedTensor::from_i32(&audio_length, &[1])
            .map_err(|error| CoreMlCtcError::Inference(error.to_string()))?;

        let preprocessor_outputs = self
            .preprocessor
            .predict(&[("audio_signal", &audio), ("audio_length", &audio_length)])
            .map_err(|error| CoreMlCtcError::Inference(error.to_string()))?;
        let (mel, mel_shape) = preprocessor_outputs
            .get_f32("mel")
            .map_err(|error| CoreMlCtcError::Inference(error.to_string()))?;
        let (mel_length, mel_length_shape) = preprocessor_outputs
            .get_i32("mel_length")
            .map_err(|error| CoreMlCtcError::Inference(error.to_string()))?;
        let mel_tensor = BorrowedTensor::from_f32(&mel, &mel_shape)
            .map_err(|error| CoreMlCtcError::Inference(error.to_string()))?;
        let mel_length_tensor = BorrowedTensor::from_i32(&mel_length, &mel_length_shape)
            .map_err(|error| CoreMlCtcError::Inference(error.to_string()))?;

        let encoder_outputs = self
            .encoder
            .predict(&[
                ("audio_signal", &mel_tensor),
                ("length", &mel_length_tensor),
            ])
            .map_err(|error| CoreMlCtcError::Inference(error.to_string()))?;
        let (encoded_length, _) = encoder_outputs
            .get_i32("encoded_length")
            .map_err(|error| CoreMlCtcError::Inference(error.to_string()))?;
        let encoded_length = encoded_length
            .first()
            .copied()
            .ok_or_else(|| CoreMlCtcError::Inference("Missing encoded_length value".to_string()))?;
        let (encoder_output, encoder_output_shape) = encoder_outputs
            .get_f32("encoder_output")
            .map_err(|error| CoreMlCtcError::Inference(error.to_string()))?;
        let encoder_output_tensor =
            BorrowedTensor::from_f32(&encoder_output, &encoder_output_shape)
                .map_err(|error| CoreMlCtcError::Inference(error.to_string()))?;

        let decoder_outputs = self
            .decoder
            .predict(&[("encoder_output", &encoder_output_tensor)])
            .map_err(|error| CoreMlCtcError::Inference(error.to_string()))?;
        let (logits, shape) = decoder_outputs
            .get_f32("ctc_logits")
            .map_err(|error| CoreMlCtcError::Inference(error.to_string()))?;
        if shape.len() != 3 || shape[0] != 1 {
            return Err(CoreMlCtcError::Inference(format!(
                "Unexpected CTC logits shape: {shape:?}"
            )));
        }

        decode_ctc_logits(
            &logits,
            shape[1],
            shape[2],
            encoded_length.max(0) as usize,
            &self.vocab,
            PARAKEET_CTC_ZH_CN_BLANK_ID,
        )
        .map_err(CoreMlCtcError::Inference)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires the separately downloaded CoreML model"]
    fn full_pipeline_smoke_test() {
        let model_dir = std::env::var("PARAKEET_CTC_COREML_MODEL_DIR")
            .expect("PARAKEET_CTC_COREML_MODEL_DIR must point to the downloaded model");
        let samples = match std::env::var("PARAKEET_CTC_AUDIO_F32LE") {
            Ok(path) => fs::read(path)
                .expect("failed to read test audio")
                .chunks_exact(4)
                .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
                .collect(),
            Err(_) => vec![0.0; 16_000],
        };

        let model =
            CoreMlCtcModel::new(Path::new(&model_dir)).expect("failed to load CoreML model");
        let result = model
            .transcribe_samples(samples)
            .expect("CoreML pipeline inference failed");

        if std::env::var_os("PARAKEET_CTC_AUDIO_F32LE").is_some() {
            assert!(
                !result.text.trim().is_empty(),
                "test audio produced no text"
            );
        }
    }
}
