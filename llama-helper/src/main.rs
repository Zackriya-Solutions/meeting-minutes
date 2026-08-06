use std::io::{self, BufRead, Write};
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::pin::pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine as _;
use encoding_rs;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::mtmd::{MtmdBitmap, MtmdContext, MtmdContextParams, MtmdInputText};
use serde::{Deserialize, Serialize};

const B64: base64::engine::general_purpose::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// Sample rate the audio projector expects, and what the pipeline delivers.
const AUDIO_SAMPLE_RATE: u32 = 16_000;

// ============================================================================
// Protocol Messages (JSON over stdin/stdout)
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Request {
    Generate {
        prompt: String,
        max_tokens: Option<i32>,
        context_size: Option<u32>,
        model_path: Option<String>,
        /// Multimodal projector for `audio_b64`. Required whenever audio is sent.
        mmproj_path: Option<String>,
        /// 16 kHz mono PCM as base64-encoded f32 little-endian bytes. When present,
        /// `prompt` must contain the media marker (`<__media__>`) once.
        audio_b64: Option<String>,
        // Sampling parameters
        temperature: Option<f32>,
        top_k: Option<i32>,
        top_p: Option<f32>,
        presence_penalty: Option<f32>,
        frequency_penalty: Option<f32>,
        repeat_penalty: Option<f32>,
        penalty_last_n: Option<i32>,
        stop_tokens: Option<Vec<String>>,
    },
    Ping,
    Shutdown,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Response {
    Response { text: String, error: Option<String> },
    Pong,
    Goodbye,
    Error { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SamplingConfig {
    temperature: f32,
    top_k: i32,
    top_p: f32,
    presence_penalty: f32,
    frequency_penalty: f32,
    repeat_penalty: f32,
    penalty_last_n: i32,
}

impl SamplingConfig {
    fn from_request(
        temperature: Option<f32>,
        top_k: Option<i32>,
        top_p: Option<f32>,
        presence_penalty: Option<f32>,
        frequency_penalty: Option<f32>,
        repeat_penalty: Option<f32>,
        penalty_last_n: Option<i32>,
    ) -> Self {
        let temperature = temperature.unwrap_or(1.0);
        let temperature = if temperature.is_finite() {
            temperature.max(0.0)
        } else {
            0.0
        };
        let top_k = top_k.unwrap_or(64).max(1);
        let top_p = top_p.unwrap_or(0.95);
        let top_p = if top_p.is_finite() && top_p > 0.0 && top_p <= 1.0 {
            top_p
        } else {
            1.0
        };
        let presence_penalty = presence_penalty.unwrap_or(0.0);
        let presence_penalty = if presence_penalty.is_finite() {
            presence_penalty.max(0.0)
        } else {
            0.0
        };
        let frequency_penalty = frequency_penalty.unwrap_or(0.0);
        let frequency_penalty = if frequency_penalty.is_finite() {
            frequency_penalty.max(0.0)
        } else {
            0.0
        };
        let repeat_penalty = repeat_penalty.unwrap_or(1.0);
        let repeat_penalty = if repeat_penalty.is_finite() && repeat_penalty > 0.0 {
            repeat_penalty
        } else {
            1.0
        };
        let penalty_last_n = penalty_last_n.unwrap_or(0).max(0);

        Self {
            temperature,
            top_k,
            top_p,
            presence_penalty,
            frequency_penalty,
            repeat_penalty,
            penalty_last_n,
        }
    }

    fn uses_penalties(&self) -> bool {
        self.penalty_last_n > 0
            && (self.presence_penalty > 0.0
                || self.frequency_penalty > 0.0
                || (self.repeat_penalty - 1.0).abs() > f32::EPSILON)
    }
}

/// Conservative thread count: max(1, (cores / 2) + 2), so the UI thread is never starved.
fn default_threads() -> i32 {
    std::thread::available_parallelism()
        .map(|n| {
            let cores = n.get() as i32;
            ((cores / 2) + 2).max(1)
        })
        .unwrap_or(2)
}

/// Decode base64 f32 little-endian PCM into samples.
///
/// This is the only lossy-by-accident step between the audio pipeline and the
/// projector: a wrong endianness or a truncated tail yields plausible-looking
/// noise rather than an error, so the length is checked explicitly.
fn audio_from_b64(encoded: &str) -> Result<Vec<f32>> {
    let bytes = B64
        .decode(encoded)
        .context("audio_b64 is not valid base64")?;
    if bytes.is_empty() {
        bail!("audio_b64 decoded to zero samples");
    }
    if bytes.len() % 4 != 0 {
        bail!(
            "audio_b64 decoded to {} bytes, which is not a whole number of f32 samples",
            bytes.len()
        );
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

/// Encode f32 samples as base64 f32 little-endian PCM — the inverse of
/// [`audio_from_b64`], used by the self-test and by the round-trip test.
fn audio_to_b64(samples: &[f32]) -> String {
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    B64.encode(bytes)
}

// ============================================================================
// VRAM Detection and GPU Layer Calculation
// ============================================================================

/// Detect available VRAM in GB
fn detect_vram_gb() -> f32 {
    #[cfg(feature = "metal")]
    {
        // macOS Metal: Query recommended max working set size
        if let Some(vram) = detect_metal_vram() {
            eprintln!("Metal VRAM detected: {:.2} GB", vram);
            return vram;
        }
    }

    #[cfg(feature = "cuda")]
    {
        // NVIDIA CUDA: Query device memory
        if let Some(vram) = detect_cuda_vram() {
            eprintln!("CUDA VRAM detected: {:.2} GB", vram);
            return vram;
        }
    }

    /// TODO: Vulkan VRAM detection

    eprintln!("VRAM detection not available, using conservative estimate");
    4.0 // Conservative fallback
}

#[cfg(feature = "metal")]
fn detect_metal_vram() -> Option<f32> {
    if let Ok(output) = std::process::Command::new("sysctl")
        .arg("hw.memsize")
        .output()
    {
        if let Ok(stdout) = String::from_utf8(output.stdout) {
            if let Some(bytes_str) = stdout.split(':').nth(1) {
                if let Ok(bytes) = bytes_str.trim().parse::<u64>() {
                    let gb = bytes as f32 / (1024.0 * 1024.0 * 1024.0);
                    // Assume GPU can use ~60% of system memory on Apple Silicon
                    return Some(gb * 0.6);
                }
            }
        }
    }
    None
}

#[cfg(feature = "cuda")]
fn detect_cuda_vram() -> Option<f32> {
    // Use nvidia-smi to query VRAM
    if let Ok(output) = std::process::Command::new("nvidia-smi")
        .args(&["--query-gpu=memory.free", "--format=csv,noheader,nounits"])
        .output()
    {
        if let Ok(stdout) = String::from_utf8(output.stdout) {
            if let Ok(mb) = stdout.trim().parse::<f32>() {
                return Some(mb / 1024.0); // Convert MB to GB
            }
        }
    }
    None
}

/// Calculate safe GPU layer count based on VRAM, model file size, and context size
fn calculate_gpu_layers(
    model_path: &PathBuf,
    model_layers: u32,
    vram_gb: f32,
    context_size: u32,
) -> u32 {
    let file_size_gb = std::fs::metadata(model_path)
        .map(|m| m.len() as f32 / 1024.0 / 1024.0 / 1024.0)
        .unwrap_or(0.0);

    if file_size_gb == 0.0 {
        eprintln!("⚠️ Could not determine model file size, using conservative default");
        return 0;
    }

    // Heuristic: Estimate KV cache size
    // 7B models (approx > 2.5GB) usually have 4096 hidden dim -> ~256MB per 1k context
    // 1B models (approx < 2.5GB) usually have 2048 hidden dim -> ~128MB per 1k context
    let kv_per_1k_gb = if file_size_gb > 2.5 { 0.25 } else { 0.12 };
    let total_kv_gb = (context_size as f32 / 1000.0) * kv_per_1k_gb;

    // Safety buffer (500MB) for OS/Display
    let safe_vram = vram_gb - 0.5;

    // For debugging
    eprintln!("📊 VRAM Analysis:");
    eprintln!("   • Available: {:.2} GB", vram_gb);
    eprintln!("   • Safe Limit: {:.2} GB", safe_vram);
    eprintln!("   • Model Weights: {:.2} GB", file_size_gb);
    eprintln!(
        "   • KV Cache ({} ctx): {:.2} GB",
        context_size, total_kv_gb
    );

    if safe_vram <= 0.0 {
        eprintln!("⚠️ No safe VRAM available, using CPU only");
        return 0;
    }

    // Calculate cost per layer
    let weight_per_layer = file_size_gb / model_layers as f32;
    let kv_per_layer = total_kv_gb / model_layers as f32;
    let total_per_layer = weight_per_layer + kv_per_layer;

    // Calculate how many layers fit
    let safe_layers = (safe_vram / total_per_layer).floor() as u32;
    let layers = safe_layers.min(model_layers);

    eprintln!(
        "   • Cost per layer: {:.2} MB (Weights) + {:.2} MB (KV) = {:.2} MB",
        weight_per_layer * 1024.0,
        kv_per_layer * 1024.0,
        total_per_layer * 1024.0
    );

    if layers < model_layers {
        eprintln!(
            "⚠️ Memory constrained. Offloading {}/{} layers ({:.1}%)",
            layers,
            model_layers,
            (layers as f32 / model_layers as f32) * 100.0
        );
    } else {
        eprintln!("✅ Full offload possible ({} layers)", layers);
    }

    layers
}

/// Get default GPU layer count with smart detection
fn get_default_gpu_layers(model_path: &PathBuf, context_size: u32) -> u32 {
    let vram = detect_vram_gb();
    // TODO: Use actual model metadata instead of heuristics
    // Heuristic: Estimate total layers based on file size
    // 7B models (Q4) are ~4.1GB and have ~32-35 layers
    // 1B models (Q4) are ~1.1GB and have ~20-28 layers
    let file_size_gb = std::fs::metadata(model_path)
        .map(|m| m.len() as f32 / 1024.0 / 1024.0 / 1024.0)
        .unwrap_or(0.0);

    let estimated_layers = if file_size_gb > 2.5 { 33 } else { 28 };

    calculate_gpu_layers(model_path, estimated_layers, vram, context_size)
}

// ============================================================================
// Model State Management
// ============================================================================

struct ModelState {
    backend: LlamaBackend,
    model: Option<LlamaModel>,
    model_path: Option<PathBuf>,
    /// Multimodal projector, when one has been loaded. Holds a raw pointer into
    /// `model`, so it MUST be dropped before `model` is replaced.
    mtmd: Option<MtmdContext>,
    mmproj_path: Option<PathBuf>,
    context_size: u32,
    last_activity: Arc<AtomicU64>,
}

impl ModelState {
    fn new() -> Result<Self> {
        let backend = LlamaBackend::init().context("Failed to init LlamaBackend")?;
        Ok(Self {
            backend,
            model: None,
            model_path: None,
            mtmd: None,
            mmproj_path: None,
            context_size: 2048,
            last_activity: Arc::new(AtomicU64::new(Self::current_timestamp())),
        })
    }

    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn update_activity(&self) {
        self.last_activity
            .store(Self::current_timestamp(), Ordering::SeqCst);
    }

    fn seconds_since_activity(&self) -> u64 {
        Self::current_timestamp() - self.last_activity.load(Ordering::SeqCst)
    }

    fn load_model_if_needed(
        &mut self,
        model_path: PathBuf,
        context_size: u32,
        mmproj_path: Option<PathBuf>,
    ) -> Result<()> {
        // Check if model is already loaded
        if let Some(ref loaded_path) = self.model_path {
            if loaded_path == &model_path && self.context_size == context_size {
                eprintln!("✓ Model already loaded");
                // The projector can still change under an unchanged model: a summary
                // request omits mmproj_path, a transcription request supplies it.
                // Only reload when it actually differs, since it costs ~1 GB of reads.
                if mmproj_path.is_some() && mmproj_path != self.mmproj_path {
                    self.load_projector(mmproj_path)?;
                }
                self.update_activity();
                return Ok(());
            }
        }

        eprintln!("📥 Loading model: {}", model_path.display());

        // Detect GPU layers
        let gpu_layers = get_default_gpu_layers(&model_path, context_size);

        // Configure model parameters with GPU offload
        let model_params = LlamaModelParams::default().with_n_gpu_layers(gpu_layers);
        let model_params = pin!(model_params);

        let model = LlamaModel::load_from_file(&self.backend, model_path.clone(), &model_params)
            .with_context(|| format!("unable to load model at {:?}", model_path))?;

        // Drop the old projector before the model it points into.
        self.mtmd = None;
        self.mmproj_path = None;

        self.model = Some(model);
        self.model_path = Some(model_path);
        self.context_size = context_size;

        if mmproj_path.is_some() {
            self.load_projector(mmproj_path)?;
        }
        self.update_activity();

        eprintln!("✅ Model loaded successfully");
        Ok(())
    }

    /// Load a multimodal projector against the currently loaded model.
    fn load_projector(&mut self, mmproj_path: Option<PathBuf>) -> Result<()> {
        // Drop first: the existing context points into the model we are about to
        // hand out another pointer to.
        self.mtmd = None;
        self.mmproj_path = None;

        let Some(path) = mmproj_path else {
            return Ok(());
        };
        let model = self
            .model
            .as_ref()
            .context("cannot load a projector before a model")?;

        eprintln!("📥 Loading projector: {}", path.display());
        let params = MtmdContextParams {
            use_gpu: true,
            print_timings: false,
            n_threads: default_threads(),
            ..Default::default()
        };
        let path_str = path
            .to_str()
            .with_context(|| format!("projector path is not valid UTF-8: {:?}", path))?;
        let mtmd = MtmdContext::init_from_file(path_str, model, &params)
            .map_err(|e| anyhow!("failed to load projector at {:?}: {e}", path))?;

        eprintln!(
            "✅ Projector loaded (audio: {}, vision: {}, sample rate: {:?})",
            mtmd.support_audio(),
            mtmd.support_vision(),
            mtmd.get_audio_sample_rate()
        );

        self.mtmd = Some(mtmd);
        self.mmproj_path = Some(path);
        Ok(())
    }

    fn generate(
        &mut self,
        prompt: String,
        audio: Option<Vec<f32>>,
        max_tokens: i32,
        sampling: SamplingConfig,
        stop_tokens: Vec<String>,
    ) -> Result<String> {
        let start_time = Instant::now();
        let model = self.model.as_ref().context("Model not loaded")?;

        let threads = default_threads();

        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(
                NonZeroU32::new(self.context_size).context("Invalid ctx size")?,
            ))
            .with_n_batch(self.context_size)
            .with_n_threads(threads)
            .with_n_threads_batch(threads);

        let mut ctx = model
            .new_context(&self.backend, ctx_params)
            .context("unable to create the llama_context")?;

        // Use context size for batch capacity to handle long prompts
        let batch_size = self.context_size as usize;
        let mut batch = LlamaBatch::new(batch_size, 1);

        let n_prompt_tokens = match audio {
            // Audio path: mtmd splits the prompt around the media marker, encodes the
            // PCM through the projector, and decodes text + audio embeddings in order.
            // `batch` deliberately stays empty — n_tokens() - 1 == -1 below, which is
            // llama.cpp's "logits of the last decoded token", i.e. what eval_chunks left.
            Some(samples) => {
                let mtmd = self
                    .mtmd
                    .as_ref()
                    .context("audio was sent but no projector is loaded (mmproj_path missing)")?;
                if !mtmd.support_audio() {
                    bail!("the loaded projector has no audio encoder");
                }

                let bitmap = MtmdBitmap::from_audio_data(&samples)
                    .map_err(|e| anyhow!("failed to wrap audio samples: {e}"))?;
                let chunks = mtmd
                    .tokenize(
                        MtmdInputText {
                            text: prompt,
                            add_special: true,
                            parse_special: true,
                        },
                        &[&bitmap],
                    )
                    .map_err(|e| anyhow!("failed to tokenize audio prompt: {e}"))?;

                eprintln!(
                    "📝 Tokenized audio prompt: {} chunks, {} tokens ({:.1}s of audio)",
                    chunks.len(),
                    chunks.total_tokens(),
                    samples.len() as f32 / AUDIO_SAMPLE_RATE as f32,
                );

                chunks
                    .eval_chunks(mtmd, &ctx, 0, 0, self.context_size as i32, true)
                    .map_err(|e| anyhow!("failed to evaluate audio prompt: {e}"))?
            }
            None => {
                let tokens_list = model
                    .str_to_token(&prompt, AddBos::Always)
                    .with_context(|| "failed to tokenize prompt")?;

                eprintln!("📝 Tokenized prompt: {} tokens", tokens_list.len());

                let last_index: i32 = (tokens_list.len() - 1) as i32;
                for (i, token) in (0_i32..).zip(tokens_list.into_iter()) {
                    let is_last = i == last_index;
                    batch
                        .add(token, i, &[0], is_last)
                        .context("Failed to add token to batch")?;
                }

                ctx.decode(&mut batch).context("llama_decode() failed")?;
                batch.n_tokens()
            }
        };
        let prompt_time = start_time.elapsed();

        let mut n_cur = n_prompt_tokens;
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut output = String::new();

        eprintln!("🔄 Starting generation (max_tokens: {})", max_tokens);

        use llama_cpp_2::sampling::LlamaSampler;

        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u32;
        let sampler = if sampling.temperature <= 0.0 {
            if sampling.uses_penalties() {
                LlamaSampler::chain_simple([
                    LlamaSampler::penalties(
                        sampling.penalty_last_n,
                        sampling.repeat_penalty,
                        sampling.frequency_penalty,
                        sampling.presence_penalty,
                    ),
                    LlamaSampler::greedy(),
                ])
            } else {
                LlamaSampler::chain_simple([LlamaSampler::greedy()])
            }
        } else if sampling.uses_penalties() {
            LlamaSampler::chain_simple([
                LlamaSampler::penalties(
                    sampling.penalty_last_n,
                    sampling.repeat_penalty,
                    sampling.frequency_penalty,
                    sampling.presence_penalty,
                ),
                LlamaSampler::top_k(sampling.top_k),
                LlamaSampler::top_p(sampling.top_p, 1),
                LlamaSampler::temp(sampling.temperature),
                LlamaSampler::dist(seed),
            ])
        } else {
            LlamaSampler::chain_simple([
                LlamaSampler::top_k(sampling.top_k),
                LlamaSampler::top_p(sampling.top_p, 1),
                LlamaSampler::temp(sampling.temperature),
                LlamaSampler::dist(seed),
            ])
        };
        let mut sampler = pin!(sampler);

        loop {
            // Check if we've generated enough tokens
            if (n_cur - n_prompt_tokens) >= max_tokens {
                eprintln!("✓ Reached max_tokens limit");
                break;
            }

            let token = sampler.as_mut().sample(&ctx, batch.n_tokens() - 1);
            sampler.as_mut().accept(token);

            if model.is_eog_token(token) {
                eprintln!(
                    "✓ End-of-generation token reached (generated {} chars)",
                    output.len()
                );
                break;
            }

            let output_bytes = match model.token_to_piece_bytes(token, 32, true, None) {
                Err(llama_cpp_2::TokenToStringError::InsufficientBufferSpace(size)) => {
                    let required_size: usize = size
                        .checked_neg()
                        .context("Invalid token piece buffer size")?
                        .try_into()
                        .context("Invalid token piece buffer size")?;
                    model.token_to_piece_bytes(token, required_size, true, None)
                }
                result => result,
            }
            .context("Failed to convert token to bytes")?;

            let mut token_text = String::with_capacity(32);
            let _ = decoder.decode_to_string(&output_bytes, &mut token_text, false);
            output.push_str(&token_text);

            // Check for model-specific stop tokens
            let mut should_stop = false;
            for stop_token in &stop_tokens {
                if output.contains(stop_token) {
                    eprintln!(
                        "✓ Stop token '{}' detected (generated {} chars)",
                        stop_token,
                        output.len()
                    );
                    // Remove the stop token from output
                    output = output.replace(stop_token, "").trim_end().to_string();
                    should_stop = true;
                    break;
                }
            }
            if should_stop {
                break;
            }

            batch.clear();
            batch
                .add(token, n_cur, &[0], true)
                .context("Failed to add generated token to batch")?;
            n_cur += 1;
            ctx.decode(&mut batch).context("failed to eval")?;
        }

        // Generation statistics
        let total_time = start_time.elapsed();
        let gen_time = total_time.saturating_sub(prompt_time);
        let output_tokens = (n_cur - n_prompt_tokens) as u64;
        let prompt_tokens = n_prompt_tokens as u64;

        let tokens_per_sec = if gen_time.as_secs_f64() > 0.0 {
            output_tokens as f64 / gen_time.as_secs_f64()
        } else {
            0.0
        };

        eprintln!("📊 Generation Statistics:");
        eprintln!("   • Prompt tokens: {}", prompt_tokens);
        eprintln!("   • Output tokens: {}", output_tokens);
        eprintln!("   • Prompt processing: {:.2}s", prompt_time.as_secs_f64());
        eprintln!("   • Generation time: {:.2}s", gen_time.as_secs_f64());
        eprintln!("   • Total time: {:.2}s", total_time.as_secs_f64());
        eprintln!("   • Speed: {:.2} tokens/sec", tokens_per_sec);

        self.update_activity();
        Ok(output)
    }
}

// ============================================================================
// Main Loop with Keep-Alive Protocol
// ============================================================================

fn send_response(response: &Response) -> Result<()> {
    let json = serde_json::to_string(response)?;
    println!("{}", json);
    io::stdout().flush()?;
    Ok(())
}

/// Prove the audio path end to end: projector loads, reports an audio encoder,
/// PCM survives the wire format, and mtmd encode + decode actually runs.
///
/// Needs a real model on disk, so it is a hand-run check rather than a test:
///   llama-helper --selftest-audio <model.gguf> <mmproj.gguf>
fn selftest_audio(model_path: &str, mmproj_path: &str) -> Result<()> {
    let mut state = ModelState::new()?;
    state.load_model_if_needed(
        PathBuf::from(model_path),
        4096,
        Some(PathBuf::from(mmproj_path)),
    )?;

    let mtmd = state.mtmd.as_ref().context("projector failed to load")?;
    assert!(
        mtmd.support_audio(),
        "projector has no audio encoder — wrong mmproj file"
    );
    assert_eq!(
        mtmd.get_audio_sample_rate(),
        Some(AUDIO_SAMPLE_RATE),
        "projector expects a different sample rate than the pipeline delivers"
    );

    // One second of 440 Hz. Not speech, so the text is meaningless — what is being
    // asserted is that the audio reaches the encoder and decodes without error.
    let samples: Vec<f32> = (0..AUDIO_SAMPLE_RATE)
        .map(|i| {
            (i as f32 * 440.0 * std::f32::consts::TAU / AUDIO_SAMPLE_RATE as f32).sin() * 0.3
        })
        .collect();

    let decoded = audio_from_b64(&audio_to_b64(&samples))?;
    assert_eq!(decoded, samples, "PCM did not survive the wire format");

    let chunks = mtmd
        .tokenize(
            MtmdInputText {
                text: gemma_audio_prompt("Transcribe this audio."),
                add_special: true,
                parse_special: true,
            },
            &[&MtmdBitmap::from_audio_data(&decoded)
                .map_err(|e| anyhow!("failed to wrap audio: {e}"))?],
        )
        .map_err(|e| anyhow!("tokenize failed: {e}"))?;

    let audio_chunks = (0..chunks.len())
        .filter_map(|i| chunks.get(i))
        .filter(|c| c.chunk_type() == llama_cpp_2::mtmd::MtmdInputChunkType::Audio)
        .count();
    assert!(
        audio_chunks >= 1,
        "tokenize produced no audio chunk — the media marker never matched"
    );

    let text = state.generate(
        gemma_audio_prompt("Transcribe this audio."),
        Some(decoded),
        32,
        SamplingConfig::from_request(Some(0.0), None, None, None, None, None, None),
        vec![],
    )?;

    println!("✅ selftest-audio passed ({} audio chunk(s))", audio_chunks);
    println!("   model output: {:?}", text.trim());
    Ok(())
}

/// Wrap an instruction in Gemma's turn format with the media marker in the user turn.
fn gemma_audio_prompt(instruction: &str) -> String {
    format!(
        "<start_of_turn>user\n{}\n{}<end_of_turn>\n<start_of_turn>model\n",
        llama_cpp_2::mtmd::mtmd_default_marker(),
        instruction
    )
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--selftest-audio") {
        let model = args.get(2).context("usage: --selftest-audio <model> <mmproj>")?;
        let mmproj = args.get(3).context("usage: --selftest-audio <model> <mmproj>")?;
        return selftest_audio(model, mmproj);
    }

    // Get idle timeout from environment variable (default 5 minutes)
    let idle_timeout_secs = std::env::var("LLAMA_IDLE_TIMEOUT")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(300); // 5 minutes default

    eprintln!(
        "🦙 llama-helper starting (idle timeout: {}s)",
        idle_timeout_secs
    );

    let mut state = ModelState::new()?;

    let stdin = io::stdin();
    let mut stdin_lock = stdin.lock();
    let mut buffer = String::new();

    loop {
        // Check idle timeout
        if state.seconds_since_activity() > idle_timeout_secs {
            eprintln!("💤 Idle timeout reached, shutting down");
            send_response(&Response::Goodbye)?;
            break;
        }

        // Read line from stdin
        buffer.clear();
        match stdin_lock.read_line(&mut buffer) {
            Ok(0) => {
                // EOF reached
                eprintln!("📪 EOF received, shutting down");
                break;
            }
            Ok(_) => {
                let line = buffer.trim();
                if line.is_empty() {
                    continue;
                }

                // Parse request
                match serde_json::from_str::<Request>(line) {
                    Ok(Request::Generate {
                        prompt,
                        max_tokens,
                        context_size,
                        model_path,
                        mmproj_path,
                        audio_b64,
                        temperature,
                        top_k,
                        top_p,
                        presence_penalty,
                        frequency_penalty,
                        repeat_penalty,
                        penalty_last_n,
                        stop_tokens,
                    }) => {
                        let max_tokens = max_tokens.unwrap_or(512);
                        let context_size = context_size.unwrap_or(2048);

                        let sampling = SamplingConfig::from_request(
                            temperature,
                            top_k,
                            top_p,
                            presence_penalty,
                            frequency_penalty,
                            repeat_penalty,
                            penalty_last_n,
                        );
                        let stop_tokens = stop_tokens.unwrap_or_else(Vec::new);

                        let audio = match audio_b64.as_deref().map(audio_from_b64).transpose() {
                            Ok(audio) => audio,
                            Err(e) => {
                                send_response(&Response::Response {
                                    text: String::new(),
                                    error: Some(format!("Invalid audio: {}", e)),
                                })?;
                                continue;
                            }
                        };

                        // Load model if path provided
                        if let Some(path_str) = model_path {
                            let path = PathBuf::from(path_str);
                            let mmproj = mmproj_path.map(PathBuf::from);
                            if let Err(e) = state.load_model_if_needed(path, context_size, mmproj) {
                                send_response(&Response::Response {
                                    text: String::new(),
                                    error: Some(format!("Failed to load model: {}", e)),
                                })?;
                                continue;
                            }
                        }

                        // Generate response with sampling parameters
                        match state.generate(
                            prompt,
                            audio,
                            max_tokens,
                            sampling,
                            stop_tokens,
                        ) {
                            Ok(text) => {
                                send_response(&Response::Response { text, error: None })?;
                            }
                            Err(e) => {
                                send_response(&Response::Response {
                                    text: String::new(),
                                    error: Some(format!("Generation failed: {}", e)),
                                })?;
                            }
                        }
                    }
                    Ok(Request::Ping) => {
                        state.update_activity();
                        send_response(&Response::Pong)?;
                    }
                    Ok(Request::Shutdown) => {
                        eprintln!("🛑 Shutdown requested");
                        send_response(&Response::Goodbye)?;
                        break;
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to parse request: {}", e);
                        send_response(&Response::Error {
                            message: format!("Invalid request: {}", e),
                        })?;
                    }
                }
            }
            Err(e) => {
                eprintln!("❌ Error reading stdin: {}", e);
                break;
            }
        }
    }

    eprintln!("👋 llama-helper exiting");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_request_defaults_penalties_when_omitted() {
        let json = r#"{"type":"generate","prompt":"summarize","temperature":0.5,"top_k":20,"top_p":0.8}"#;
        let request: Request = serde_json::from_str(json).unwrap();
        let Request::Generate {
            temperature,
            top_k,
            top_p,
            presence_penalty,
            frequency_penalty,
            repeat_penalty,
            penalty_last_n,
            ..
        } = request else {
            panic!("expected generate request");
        };

        let sampling = SamplingConfig::from_request(
            temperature,
            top_k,
            top_p,
            presence_penalty,
            frequency_penalty,
            repeat_penalty,
            penalty_last_n,
        );

        assert_eq!(sampling.presence_penalty, 0.0);
        assert_eq!(sampling.frequency_penalty, 0.0);
        assert_eq!(sampling.repeat_penalty, 1.0);
        assert_eq!(sampling.penalty_last_n, 0);
        assert!(!sampling.uses_penalties());
    }

    #[test]
    fn audio_survives_the_base64_f32_wire_format() {
        // Values chosen to catch endianness swaps and sign/exponent truncation.
        let samples = vec![0.0, 1.0, -1.0, 0.5, -0.000_123_4, f32::MIN_POSITIVE, 0.999_999];
        assert_eq!(audio_from_b64(&audio_to_b64(&samples)).unwrap(), samples);
    }

    #[test]
    fn audio_rejects_a_truncated_tail() {
        // Three bytes is not a whole f32; silently dropping them would shift every
        // following sample and produce plausible noise instead of an error.
        let err = audio_from_b64(&B64.encode([1u8, 2, 3])).unwrap_err().to_string();
        assert!(err.contains("not a whole number of f32 samples"), "{err}");

        assert!(audio_from_b64("").is_err(), "empty audio must be rejected");
        assert!(audio_from_b64("not base64!!").is_err());
    }

    #[test]
    fn generate_request_carries_audio_and_projector() {
        let json = r#"{"type":"generate","prompt":"<__media__>","audio_b64":"AAAAAA==","mmproj_path":"/m/p.gguf"}"#;
        let Request::Generate { audio_b64, mmproj_path, .. } =
            serde_json::from_str(json).unwrap() else { panic!("expected generate") };
        assert_eq!(mmproj_path.as_deref(), Some("/m/p.gguf"));
        assert_eq!(audio_from_b64(&audio_b64.unwrap()).unwrap(), vec![0.0]);
    }

    #[test]
    fn generate_request_without_audio_stays_text_only() {
        let json = r#"{"type":"generate","prompt":"summarize"}"#;
        let Request::Generate { audio_b64, mmproj_path, .. } =
            serde_json::from_str(json).unwrap() else { panic!("expected generate") };
        assert!(audio_b64.is_none() && mmproj_path.is_none());
    }

    #[test]
    fn gemma_audio_prompt_contains_exactly_one_media_marker() {
        let prompt = gemma_audio_prompt("Transcribe this audio.");
        let marker = llama_cpp_2::mtmd::mtmd_default_marker();
        assert_eq!(prompt.matches(marker).count(), 1, "{prompt}");
        assert!(prompt.contains("<start_of_turn>model\n"));
    }

    #[test]
    fn generate_request_deserializes_qwen_penalties() {
        let json = r#"{"type":"generate","prompt":"summarize","temperature":0.5,"top_k":20,"top_p":0.8,"presence_penalty":0.3,"frequency_penalty":0.0,"repeat_penalty":1.05,"penalty_last_n":256}"#;
        let request: Request = serde_json::from_str(json).unwrap();
        let Request::Generate {
            temperature,
            top_k,
            top_p,
            presence_penalty,
            frequency_penalty,
            repeat_penalty,
            penalty_last_n,
            ..
        } = request else {
            panic!("expected generate request");
        };

        let sampling = SamplingConfig::from_request(
            temperature,
            top_k,
            top_p,
            presence_penalty,
            frequency_penalty,
            repeat_penalty,
            penalty_last_n,
        );

        assert_eq!(sampling.temperature, 0.5);
        assert_eq!(sampling.top_k, 20);
        assert_eq!(sampling.top_p, 0.8);
        assert_eq!(sampling.presence_penalty, 0.3);
        assert_eq!(sampling.frequency_penalty, 0.0);
        assert_eq!(sampling.repeat_penalty, 1.05);
        assert_eq!(sampling.penalty_last_n, 256);
        assert!(sampling.uses_penalties());
    }
}
