//! Nemotron 3.5 ASR sidecar.
//!
//! Speaks newline-delimited JSON over stdin/stdout, mirroring the llama-helper protocol.
//! Diagnostics go to stderr so stdout carries nothing but protocol messages.
//!
//! It exists as a separate process because `parakeet-rs` needs a newer `ort` than
//! meetily's VAD crate allows in the same build. See Cargo.toml for the details.

use std::io::{self, BufRead, Write};

use anyhow::{bail, Context, Result};
use base64::Engine as _;
use parakeet_rs::{ExecutionConfig, Nemotron};
#[cfg(windows)]
use parakeet_rs::ExecutionProvider;
use serde::{Deserialize, Serialize};

/// 560 ms at 16 kHz, one of the chunk sizes Nemotron's cache-aware encoder is built for.
const CHUNK_SAMPLES: usize = 8960;

const SAMPLE_RATE: usize = 16_000;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Request {
    /// Load the model from a directory holding encoder.onnx, encoder.onnx.data,
    /// decoder_joint.onnx and tokenizer.model.
    Load {
        model_dir: String,
        /// BCP-47-ish code such as "en-US" or "tr-TR"; "auto" (the default) lets the
        /// multilingual model decide for itself.
        language: Option<String>,
    },
    /// Transcribe one utterance. `audio_b64` is base64 over little-endian f32 samples,
    /// mono, 16 kHz - the format meetily's pipeline already produces.
    ///
    /// Decoder state is *kept* between calls: Nemotron's cache-aware encoder produces
    /// nothing for roughly its first second, so resetting per utterance would silently
    /// swallow every short VAD segment. Send `reset` when a new session starts.
    Transcribe { audio_b64: String },
    /// Feed one step of a continuous stream and return only what it decoded.
    ///
    /// Unlike `transcribe` the reply is *not* trimmed. Nemotron emits
    /// SentencePiece text, where a leading space is what marks the start of a
    /// word; trimming each piece turns "speed" + " masters" into "speed" +
    /// "masters" and the caller can no longer tell whether to join them. Callers
    /// concatenate pieces verbatim and trim once at the end.
    TranscribeStream { audio_b64: String },
    /// Clear decoder state so the next `transcribe` starts a fresh session.
    Reset,
    Ping,
    Shutdown,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Response {
    /// `provider` is what was *requested*. parakeet-rs pairs every GPU provider with a
    /// CPU fallback internally, so this is not proof the GPU is in use.
    Loaded { provider: String },
    Transcript { text: String },
    /// One streaming step's output, verbatim. Empty when the step produced no
    /// tokens, which is normal: the encoder needs a whole 560 ms before it emits.
    Piece { text: String },
    Pong,
    Goodbye,
    Error { message: String },
}

fn main() -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut model: Option<Nemotron> = None;

    for line in stdin.lock().lines() {
        let line = line.context("failed to read from stdin")?;
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<Request>(&line) {
            Ok(Request::Shutdown) => {
                write_response(&mut stdout, &Response::Goodbye)?;
                return Ok(());
            }
            Ok(request) => handle(request, &mut model)
                .unwrap_or_else(|e| Response::Error { message: format!("{e:#}") }),
            Err(e) => Response::Error { message: format!("malformed request: {e}") },
        };

        write_response(&mut stdout, &response)?;
    }

    Ok(())
}

fn handle(request: Request, model: &mut Option<Nemotron>) -> Result<Response> {
    match request {
        Request::Ping => Ok(Response::Pong),
        Request::Shutdown => Ok(Response::Goodbye),
        Request::Load { model_dir, language } => {
            let (loaded, provider) = load(&model_dir, language.as_deref())?;
            *model = Some(loaded);
            Ok(Response::Loaded { provider })
        }
        Request::Transcribe { audio_b64 } => {
            let model = model
                .as_mut()
                .context("no model loaded; send a `load` request first")?;
            let samples = decode_samples(&audio_b64)?;
            Ok(Response::Transcript { text: transcribe(model, &samples)? })
        }
        Request::TranscribeStream { audio_b64 } => {
            let model = model
                .as_mut()
                .context("no model loaded; send a `load` request first")?;
            let samples = decode_samples(&audio_b64)?;
            let text = model
                .transcribe_chunk(&samples)
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("transcribe_chunk failed")?;
            Ok(Response::Piece { text })
        }
        Request::Reset => {
            if let Some(model) = model.as_mut() {
                model.reset();
            }
            Ok(Response::Pong)
        }
    }
}

fn load(model_dir: &str, language: Option<&str>) -> Result<(Nemotron, String)> {
    #[cfg(windows)]
    let (config, provider) = (
        ExecutionConfig::new().with_execution_provider(ExecutionProvider::DirectML),
        "DirectML",
    );
    #[cfg(not(windows))]
    let (config, provider) = (ExecutionConfig::new(), "CPU");

    let mut model = Nemotron::from_pretrained(model_dir, Some(config))
        .map_err(|e| anyhow::anyhow!("{e}"))
        .with_context(|| format!("failed to load Nemotron from {model_dir}"))?;

    // Only the multilingual variant accepts a target language. Treat a rejection as a
    // note rather than a failure so the English-only variant still loads.
    if let Some(lang) = language {
        if let Err(e) = model.set_target_lang(lang) {
            eprintln!("nemotron-helper: could not set language '{lang}': {e}");
        }
    }

    eprintln!("nemotron-helper: loaded {model_dir} requesting {provider}");
    Ok((model, provider.to_string()))
}

fn transcribe(model: &mut Nemotron, samples: &[f32]) -> Result<String> {
    // Deliberately no reset here - see the `Transcribe` doc comment. Collect what each
    // chunk emits rather than reading get_transcript(), which returns everything decoded
    // since the last reset and would re-send earlier utterances on every call.
    let mut text = String::new();

    for chunk in samples.chunks(CHUNK_SAMPLES) {
        let piece = model
            .transcribe_chunk(chunk)
            .map_err(|e| anyhow::anyhow!("{e}"))
            .context("transcribe_chunk failed")?;
        text.push_str(&piece);
    }

    Ok(text.trim().to_string())
}

fn decode_samples(audio_b64: &str) -> Result<Vec<f32>> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(audio_b64)
        .context("audio_b64 is not valid base64")?;

    if bytes.len() % 4 != 0 {
        bail!("audio payload is {} bytes, not a whole number of f32 samples", bytes.len());
    }

    let samples: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();

    if samples.len() < SAMPLE_RATE / 10 {
        bail!("audio is {} samples, shorter than the 100 ms minimum", samples.len());
    }

    Ok(samples)
}

fn write_response(stdout: &mut io::Stdout, response: &Response) -> Result<()> {
    serde_json::to_writer(&mut *stdout, response)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}
