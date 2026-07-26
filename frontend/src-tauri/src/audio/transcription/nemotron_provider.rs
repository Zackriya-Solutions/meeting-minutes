// audio/transcription/nemotron_provider.rs
//
// Transcription provider backed by the `nemotron-helper` sidecar process.
//
// Nemotron 3.5 ASR runs out of process because `parakeet-rs` needs `ort 2.0.0-rc.12`
// while this crate's VAD dependency (`silero_rs`) pins `ort` to exactly `2.0.0-rc.10`.
// The sidecar carries its own lockfile, so the two ONNX Runtime versions never meet.
// The protocol is newline-delimited JSON over stdin/stdout, matching llama-helper.

use super::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use async_trait::async_trait;
use base64::Engine as _;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use tokio::sync::Mutex;

/// Model directory name under `<app data>/models/nemotron/`.
pub const DEFAULT_NEMOTRON_MODEL: &str = "nemotron-3.5-asr-streaming-0.6b";

/// 100 ms at 16 kHz. Shorter clips carry no usable speech and make the encoder unhappy.
const MIN_SAMPLES: usize = 1600;

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Request<'a> {
    Load { model_dir: &'a str, language: Option<&'a str> },
    Transcribe { audio_b64: String },
    /// One step of a continuous stream. The reply is not trimmed - see
    /// `TranscriptionProvider::transcribe_step`.
    TranscribeStream { audio_b64: String },
    Shutdown,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Response {
    Loaded { provider: String },
    Transcript { text: String },
    Piece { text: String },
    Pong,
    Goodbye,
    Error { message: String },
}

/// The running sidecar plus the pipes used to talk to it.
struct Sidecar {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        // Ask politely first so the helper can release the GPU session, then make sure.
        if let Ok(payload) = serde_json::to_string(&Request::Shutdown) {
            let _ = writeln!(self.stdin, "{}", payload);
            let _ = self.stdin.flush();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct NemotronProvider {
    model_dir: PathBuf,
    language: Option<String>,
    /// Spawned on first use and reused afterwards; loading the model costs ~4 s.
    sidecar: Mutex<Option<Sidecar>>,
}

impl NemotronProvider {
    pub fn new(model_dir: PathBuf, language: Option<String>) -> Self {
        Self { model_dir, language, sidecar: Mutex::new(None) }
    }

    /// Start the sidecar and load the model, unless that already happened.
    ///
    /// This has to run during engine initialisation rather than lazily on the first
    /// transcribe: the transcription worker checks `is_model_loaded()` before it will
    /// dispatch a chunk, so a provider that only becomes loaded *through* transcribing
    /// would never load at all - every chunk gets skipped and `transcribe` never runs.
    pub async fn ensure_started(&self) -> Result<(), String> {
        let mut guard = self.sidecar.lock().await;
        if guard.is_none() {
            *guard = Some(self.spawn()?);
        }
        Ok(())
    }

    /// Directory holding the Nemotron ONNX files for `model_name`.
    pub fn model_dir_for(app_data_dir: &PathBuf, model_name: &str) -> PathBuf {
        app_data_dir.join("models").join("nemotron").join(model_name)
    }

    /// True when the four files the helper needs are present.
    pub fn model_is_installed(model_dir: &PathBuf) -> bool {
        ["encoder.onnx", "encoder.onnx.data", "decoder_joint.onnx", "tokenizer.model"]
            .iter()
            .all(|f| model_dir.join(f).is_file())
    }

    /// Whether a directory entry names the sidecar executable.
    ///
    /// Matching the prefix alone is not enough: a debug build leaves nemotron-helper.pdb
    /// and nemotron-helper.d next to the binary, and read_dir returns entries in no
    /// defined order, so the scan below could otherwise hand a non-executable to
    /// Command::new.
    fn is_helper_binary(name: &str) -> bool {
        if !name.starts_with("nemotron-helper") {
            return false;
        }
        if cfg!(windows) {
            name.ends_with(".exe")
        } else {
            !name.contains('.')
        }
    }

    /// Locate the sidecar binary, mirroring how llama-helper is resolved.
    fn resolve_helper_binary() -> Result<PathBuf, String> {
        if let Ok(path) = std::env::var("MEETILY_NEMOTRON_HELPER") {
            let path = PathBuf::from(path);
            if path.is_file() {
                return Ok(path);
            }
        }

        let exe_dir = std::env::current_exe()
            .map_err(|e| format!("cannot locate the running executable: {}", e))?
            .parent()
            .ok_or_else(|| "running executable has no parent directory".to_string())?
            .to_path_buf();

        // Tauri bundles external binaries with a target-triple suffix; a plain name is
        // what a local `cargo build` produces.
        if let Ok(entries) = std::fs::read_dir(&exe_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = match path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n,
                    None => continue,
                };
                if Self::is_helper_binary(name) && path.is_file() {
                    return Ok(path);
                }
            }
        }

        Err(format!(
            "nemotron-helper not found next to {} - set MEETILY_NEMOTRON_HELPER to override",
            exe_dir.display()
        ))
    }

    fn spawn(&self) -> Result<Sidecar, String> {
        if !Self::model_is_installed(&self.model_dir) {
            return Err(format!(
                "Nemotron model files are missing from {}",
                self.model_dir.display()
            ));
        }

        let binary = Self::resolve_helper_binary()?;
        info!("Starting nemotron-helper: {}", binary.display());

        let mut command = Command::new(&binary);
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command
            .spawn()
            .map_err(|e| format!("failed to start nemotron-helper: {}", e))?;

        let stdin = child.stdin.take().ok_or("nemotron-helper stdin unavailable")?;
        let stdout = child.stdout.take().ok_or("nemotron-helper stdout unavailable")?;
        let mut sidecar = Sidecar { child, stdin, stdout: BufReader::new(stdout) };

        let model_dir = self.model_dir.to_string_lossy().to_string();
        let request = Request::Load {
            model_dir: &model_dir,
            language: Some(self.language.as_deref().unwrap_or("auto")),
        };

        match Self::exchange(&mut sidecar, &request)? {
            Response::Loaded { provider } => {
                info!("Nemotron loaded on the {} execution provider", provider);
                Ok(sidecar)
            }
            Response::Error { message } => Err(format!("nemotron-helper failed to load: {}", message)),
            _ => Err("unexpected reply to the load request".to_string()),
        }
    }

    /// Write one request and read exactly one reply.
    fn exchange(sidecar: &mut Sidecar, request: &Request<'_>) -> Result<Response, String> {
        let payload = serde_json::to_string(request)
            .map_err(|e| format!("failed to encode request: {}", e))?;

        writeln!(sidecar.stdin, "{}", payload)
            .and_then(|_| sidecar.stdin.flush())
            .map_err(|e| format!("failed to write to nemotron-helper: {}", e))?;

        let mut line = String::new();
        let read = sidecar
            .stdout
            .read_line(&mut line)
            .map_err(|e| format!("failed to read from nemotron-helper: {}", e))?;
        if read == 0 {
            return Err("nemotron-helper exited unexpectedly".to_string());
        }

        serde_json::from_str(&line)
            .map_err(|e| format!("unparsable reply from nemotron-helper: {} ({})", e, line.trim()))
    }
}

#[async_trait]
impl TranscriptionProvider for NemotronProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        _language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        if audio.len() < MIN_SAMPLES {
            return Err(TranscriptionError::AudioTooShort {
                samples: audio.len(),
                minimum: MIN_SAMPLES,
            });
        }

        let mut bytes = Vec::with_capacity(audio.len() * 4);
        for sample in &audio {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        let audio_b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

        let mut guard = self.sidecar.lock().await;
        if guard.is_none() {
            *guard = Some(self.spawn().map_err(TranscriptionError::EngineFailed)?);
        }

        let request = Request::Transcribe { audio_b64 };
        let result = {
            let sidecar = guard.as_mut().expect("sidecar was just created");
            Self::exchange(sidecar, &request)
        };

        match result {
            Ok(Response::Transcript { text }) => Ok(TranscriptResult {
                text: text.trim().to_string(),
                confidence: None, // the helper does not surface token probabilities
                is_partial: false,
            }),
            Ok(Response::Error { message }) => {
                Err(TranscriptionError::EngineFailed(message))
            }
            Ok(_) => Err(TranscriptionError::EngineFailed(
                "unexpected reply to a transcribe request".to_string(),
            )),
            Err(e) => {
                // A broken pipe means the helper died; drop it so the next call respawns.
                warn!("nemotron-helper call failed, restarting it next time: {}", e);
                *guard = None;
                Err(TranscriptionError::EngineFailed(e))
            }
        }
    }

    async fn is_model_loaded(&self) -> bool {
        self.sidecar.lock().await.is_some()
    }

    async fn get_current_model(&self) -> Option<String> {
        self.model_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
    }

    fn provider_name(&self) -> &'static str {
        "Nemotron 3.5 ASR"
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn transcribe_step(
        &self,
        audio: Vec<f32>,
    ) -> std::result::Result<String, TranscriptionError> {
        if audio.len() != super::provider::STREAM_STEP_SAMPLES {
            // Refused rather than truncated or padded. The encoder advances one step
            // per call whatever it is handed, so a wrong size does not fail loudly -
            // it quietly leaves audio behind and every later step inherits the drift.
            return Err(TranscriptionError::EngineFailed(format!(
                "a streaming step must be exactly {} samples, got {}",
                super::provider::STREAM_STEP_SAMPLES,
                audio.len()
            )));
        }

        let mut bytes = Vec::with_capacity(audio.len() * 4);
        for sample in &audio {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        let audio_b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

        let mut guard = self.sidecar.lock().await;
        if guard.is_none() {
            *guard = Some(self.spawn().map_err(TranscriptionError::EngineFailed)?);
        }

        let request = Request::TranscribeStream { audio_b64 };
        let result = {
            let sidecar = guard.as_mut().expect("sidecar was just created");
            Self::exchange(sidecar, &request)
        };

        match result {
            // Returned untouched: trimming here would erase the leading space that
            // separates this piece's first word from the previous piece's last.
            Ok(Response::Piece { text }) => Ok(text),
            Ok(Response::Error { message }) => Err(TranscriptionError::EngineFailed(message)),
            Ok(_) => Err(TranscriptionError::EngineFailed(
                "unexpected reply to a streaming step".to_string(),
            )),
            Err(e) => {
                // The encoder cache dies with the process, so the stream restarts
                // from scratch rather than resuming mid-sentence.
                warn!("nemotron-helper streaming call failed, restarting it next time: {}", e);
                *guard = None;
                Err(TranscriptionError::EngineFailed(e))
            }
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// Point the resolver at the binary staged for bundling, since the test harness runs
    /// from target/debug/deps where no sidecar sits next to it.
    fn use_staged_helper() {
        let staged = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join("nemotron-helper-x86_64-pc-windows-msvc.exe");
        assert!(staged.is_file(), "sidecar not staged at {}", staged.display());
        std::env::set_var("MEETILY_NEMOTRON_HELPER", &staged);
    }

    fn installed_model_dir() -> PathBuf {
        let dir = PathBuf::from(std::env::var("APPDATA").unwrap())
            .join("com.meetily.ai")
            .join("models")
            .join("nemotron")
            .join(DEFAULT_NEMOTRON_MODEL);
        assert!(
            NemotronProvider::model_is_installed(&dir),
            "model not installed at {}",
            dir.display()
        );
        dir
    }

    /// The transcription worker refuses to dispatch a chunk unless `is_model_loaded()`
    /// already reports true (see audio/transcription/worker.rs). A provider that only
    /// becomes "loaded" once `transcribe` has run can therefore never load at all: the
    /// worker skips every chunk, so `transcribe` is never called. This test pins the
    /// contract the worker depends on.
    ///
    /// Needs the model and the staged sidecar, so it is ignored by default. Run with:
    ///   cargo test -p meetily --lib audio::transcription::nemotron_provider -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn reports_loaded_after_start_without_transcribing_first() {
        use_staged_helper();
        let provider = NemotronProvider::new(installed_model_dir(), None);

        assert!(
            !provider.is_model_loaded().await,
            "nothing has been started yet"
        );

        provider.ensure_started().await.expect("sidecar should start");

        assert!(
            provider.is_model_loaded().await,
            "worker.rs gates chunk dispatch on this being true before any transcribe call"
        );
    }

    /// Feeding the encoder a buffer that is not exactly one step is silent audio loss,
    /// not a visible failure: it advances by one step per call and buries the rest. So
    /// the wrong size has to be refused, and refused *before* any I/O - which is why
    /// this test needs neither the model nor the sidecar.
    #[tokio::test]
    async fn a_streaming_step_must_be_exactly_one_step() {
        let provider = NemotronProvider::new(PathBuf::from("no-such-model"), None);

        for wrong in [
            super::super::provider::STREAM_STEP_SAMPLES - 1,
            super::super::provider::STREAM_STEP_SAMPLES + 1,
            super::super::provider::STREAM_STEP_SAMPLES * 2,
            0,
        ] {
            let error = provider
                .transcribe_step(vec![0.0; wrong])
                .await
                .expect_err("a step of the wrong length must be refused");

            assert!(
                error.to_string().contains("must be exactly"),
                "{} samples was rejected for the wrong reason: {}",
                wrong,
                error
            );
        }
    }

    /// The pipeline sends steps whatever engine is loaded, so this is what tells the
    /// worker it may use them.
    #[test]
    fn the_provider_advertises_streaming() {
        let provider = NemotronProvider::new(PathBuf::from("no-such-model"), None);
        assert!(provider.supports_streaming());
    }

    /// A debug build drops nemotron-helper.pdb and nemotron-helper.d beside the binary,
    /// and read_dir order is not defined, so a prefix match alone can hand a non-executable
    /// to Command::new and fail the spawn.
    #[test]
    fn helper_lookup_accepts_only_executables() {
        assert!(NemotronProvider::is_helper_binary("nemotron-helper.exe"));
        assert!(NemotronProvider::is_helper_binary(
            "nemotron-helper-x86_64-pc-windows-msvc.exe"
        ));

        assert!(!NemotronProvider::is_helper_binary("nemotron-helper.pdb"));
        assert!(!NemotronProvider::is_helper_binary("nemotron-helper.d"));
        assert!(!NemotronProvider::is_helper_binary("llama-helper.exe"));
    }

    /// Nemotron withholds roughly its last second of audio as look-ahead context, so it
    /// returned nothing for the ~1.5 s segments the pipeline used to produce. Now that VAD
    /// bridges pauses inside a sentence, segments are several seconds long. This checks
    /// whether that makes Nemotron usable on the segments the pipeline actually emits.
    ///
    /// Point MEETILY_TEST_WAV at a 16 kHz mono recording of real speech.
    #[tokio::test]
    #[ignore]
    async fn transcribes_the_vad_segments_the_pipeline_emits() {
        use_staged_helper();

        let wav = PathBuf::from(
            std::env::var("MEETILY_TEST_WAV")
                .expect("set MEETILY_TEST_WAV to a 16 kHz mono recording"),
        );
        let samples = read_wav_16k_mono(&wav);
        let provider = NemotronProvider::new(installed_model_dir(), None);

        for redemption_ms in [400u32, 800] {
            let segments = crate::audio::vad::get_speech_chunks(&samples, redemption_ms)
                .expect("VAD should segment the recording");

            let mut pieces = Vec::new();
            for segment in &segments {
                let result = provider
                    .transcribe(segment.samples.clone(), None)
                    .await
                    .expect("transcribe should reach the sidecar");
                if !result.text.trim().is_empty() {
                    pieces.push(result.text.trim().to_string());
                }
            }

            println!(
                "redemption {:>4}ms: {} segments -> {}",
                redemption_ms,
                segments.len(),
                pieces.join(" | ")
            );
        }
    }

    /// Decode a 16-bit PCM mono WAV into the f32 samples the pipeline uses.
    fn read_wav_16k_mono(path: &Path) -> Vec<f32> {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        // Walk the RIFF chunks rather than assuming the data starts at byte 44, which
        // only holds for the most minimal encoder output.
        let mut offset = 12; // past "RIFF" + size + "WAVE"
        let mut data: Option<&[u8]> = None;
        while offset + 8 <= bytes.len() {
            let id = &bytes[offset..offset + 4];
            let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
            let body = &bytes[offset + 8..(offset + 8 + size).min(bytes.len())];
            if id == b"fmt " {
                let channels = u16::from_le_bytes(body[2..4].try_into().unwrap());
                let rate = u32::from_le_bytes(body[4..8].try_into().unwrap());
                let bits = u16::from_le_bytes(body[14..16].try_into().unwrap());
                assert_eq!((channels, rate, bits), (1, 16_000, 16), "test wav must be 16 kHz mono 16-bit");
            } else if id == b"data" {
                data = Some(body);
                break;
            }
            offset += 8 + size + (size & 1); // chunks are word-aligned
        }

        data.expect("wav has no data chunk")
            .chunks_exact(2)
            .map(|s| i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0)
            .collect()
    }

    /// Round-trips real speech through the provider rather than the Python harness, so
    /// the base64 framing, the reply parsing and the model output are all covered on the
    /// Rust side. Override the clip with MEETILY_TEST_WAV.
    #[tokio::test]
    #[ignore]
    async fn transcribes_real_speech() {
        use_staged_helper();

        let wav = PathBuf::from(
            std::env::var("MEETILY_TEST_WAV")
                .unwrap_or_else(|_| r"C:\Work\models\jfk_norm.wav".to_string()),
        );
        let samples = read_wav_16k_mono(&wav);
        let provider = NemotronProvider::new(installed_model_dir(), None);

        let result = provider
            .transcribe(samples, None)
            .await
            .expect("transcribe should reach the sidecar and come back");

        println!("transcript: {:?}", result.text);
        let lowered = result.text.to_lowercase();
        assert!(
            lowered.contains("fellow americans") && lowered.contains("your country"),
            "unexpected transcript: {:?}",
            result.text
        );
    }
}
