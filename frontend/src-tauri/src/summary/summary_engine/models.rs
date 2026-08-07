// Model definitions and prompt templates for built-in AI summary generation
// Designed for easy extension - just add new entries to get_available_models()

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ============================================================================
// Model Definitions
// ============================================================================

/// Sampling parameters supported by the built-in AI -> llama-helper pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SamplingParams {
    /// Temperature - 0.0 triggers greedy decoding in llama-helper.
    pub temperature: f32,

    /// Top-K sampling - limits vocabulary to top K tokens.
    pub top_k: i32,

    /// Top-P (nucleus) sampling - cumulative probability threshold.
    pub top_p: f32,

    /// Presence penalty - discourages reusing tokens that already appeared in the generated output.
    pub presence_penalty: f32,

    /// Frequency penalty - discourages repeated token frequency in the generated output.
    pub frequency_penalty: f32,

    /// Repeat penalty - llama.cpp repeat penalty, 1.0 disables it.
    pub repeat_penalty: f32,

    /// Number of recent generated tokens to apply penalties over, 0 disables penalties.
    pub penalty_last_n: i32,

    /// Stop tokens - generation stops when any of these appear in output
    pub stop_tokens: Vec<String>,
}

impl SamplingParams {
    /// Gemma instruct preset — Google's recommended sampling for the -it models.
    pub fn gemma_instruct(stop_tokens: Vec<String>) -> Self {
        Self {
            temperature: 1.0,
            top_k: 64,
            top_p: 0.95,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
            repeat_penalty: 1.0,
            penalty_last_n: 0,
            stop_tokens,
        }
    }

    /// Normalize built-in presets to the subset supported by llama-helper.
    pub fn sanitize_for_llama_helper(&self) -> Self {
        let temperature = if self.temperature.is_finite() {
            self.temperature.max(0.0)
        } else {
            0.0
        };
        let top_k = self.top_k.max(0);
        let top_p = if self.top_p.is_finite() && self.top_p > 0.0 && self.top_p <= 1.0 {
            self.top_p
        } else {
            1.0
        };
        let presence_penalty = if self.presence_penalty.is_finite() {
            self.presence_penalty.max(0.0)
        } else {
            0.0
        };
        let frequency_penalty = if self.frequency_penalty.is_finite() {
            self.frequency_penalty.max(0.0)
        } else {
            0.0
        };
        let repeat_penalty = if self.repeat_penalty.is_finite() && self.repeat_penalty > 0.0 {
            self.repeat_penalty
        } else {
            1.0
        };
        let penalty_last_n = self.penalty_last_n.max(0);

        Self {
            temperature,
            top_k,
            top_p,
            presence_penalty,
            frequency_penalty,
            repeat_penalty,
            penalty_last_n,
            stop_tokens: self.stop_tokens.clone(),
        }
    }
}

/// Multimodal projector shipped alongside a model's weights.
///
/// llama.cpp keeps the audio/vision encoders in a separate GGUF from the text
/// weights, so an audio-capable model is always two files. Its presence is what
/// makes a model usable for transcription — there is no separate capability flag
/// to drift out of sync with the files on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Projector {
    /// Projector filename on disk (e.g. "mmproj-gemma-4-E2B-it-BF16.gguf")
    pub file: String,
    /// Download URL for the projector
    pub url: String,
    /// Projector size in MiB, counted separately so progress can span both files
    pub size_mb: u64,
}

/// Definition of a built-in AI model with all metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDef {
    /// Model name in format "family:variant" (e.g., "gemma4:e4b")
    /// This is what's stored in database as model field when provider="builtin-ai"
    pub name: String,

    /// Display name for UI (e.g., "Gemma 4 E4B (Audio + Text)")
    pub display_name: String,

    /// GGUF filename on disk (e.g., "gemma-4-E4B-it-Q4_0.gguf")
    pub gguf_file: String,

    /// Template name for prompt formatting (e.g., "gemma4")
    pub template: String,

    /// Download URL (HuggingFace or other source)
    pub download_url: String,

    /// File size in MiB. The field name is kept for API compatibility.
    pub size_mb: u64,

    /// Context window size in tokens (configurable per model!)
    /// This is used for chunking in processor.rs
    pub context_size: u32,

    /// Model layer count (for GPU offloading calculation)
    pub layer_count: u32,

    /// Sampling parameters for this model
    pub sampling: SamplingParams,

    /// Short description for UI
    pub description: String,

    /// Multimodal projector, when this model has one. `Some` means the model can
    /// take audio input and is offered for transcription as well as summaries.
    #[serde(default)]
    pub mmproj: Option<Projector>,
}

impl ModelDef {
    /// Whether this model accepts audio input (i.e. can transcribe).
    pub fn is_audio(&self) -> bool {
        self.mmproj.is_some()
    }

    /// Bytes the user has to download for this model: weights plus projector.
    pub fn total_size_mb(&self) -> u64 {
        self.size_mb + self.mmproj.as_ref().map_or(0, |p| p.size_mb)
    }
}

/// Get all available built-in AI models
/// Add new models here - the system will automatically detect and manage them
pub fn get_available_models() -> Vec<ModelDef> {
    vec![
        // Gemma 4 E2B - audio + text, smaller tier. Leads the list because it is
        // DEFAULT_SUMMARY_MODEL: one model serves transcription and summaries, and
        // this is the one onboarding downloads, so it is what most users have.
        //
        // BF16 projector on purpose (both tiers). The Q8_0 projector is half the
        // size, but the conformer's per-layer activations exceed their clamp
        // thresholds unevenly once quantized, degrading transcripts (llama.cpp#21421).
        ModelDef {
            name: "gemma4:e2b".to_string(),
            display_name: "Gemma 4 E2B (Audio + Text)".to_string(),
            gguf_file: "gemma-4-E2B-it-Q4_0.gguf".to_string(),
            template: "gemma4".to_string(),
            download_url: "https://huggingface.co/ggml-org/gemma-4-E2B-it-GGUF/resolve/main/gemma-4-E2B-it-Q4_0.gguf".to_string(),
            size_mb: 2710,
            // ponytail: 8192 rather than the 32768 the model supports. The sidecar
            // builds a fresh context per request, so n_ctx is paid on every live
            // audio segment. Raise it if summary chunking becomes the bottleneck.
            context_size: 8192,
            layer_count: 30,
            sampling: SamplingParams::gemma_instruct(vec!["<end_of_turn>".to_string()]),
            description: "Transcribes audio and writes summaries on modest hardware. Needs ~3.6GB of downloads.".to_string(),
            mmproj: Some(Projector {
                file: "mmproj-gemma-4-E2B-it-BF16.gguf".to_string(),
                url: "https://huggingface.co/ggml-org/gemma-4-E2B-it-GGUF/resolve/main/mmproj-gemma-4-E2B-it-BF16.gguf".to_string(),
                size_mb: 941,
            }),
        },
        // Gemma 4 E4B - the upgrade tier. Better summaries, ~1.7GB more download,
        // and it wants ~16GB of RAM to stay comfortable.
        ModelDef {
            name: "gemma4:e4b".to_string(),
            display_name: "Gemma 4 E4B (Audio + Text)".to_string(),
            gguf_file: "gemma-4-E4B-it-Q4_0.gguf".to_string(),
            template: "gemma4".to_string(),
            download_url: "https://huggingface.co/ggml-org/gemma-4-E4B-it-GGUF/resolve/main/gemma-4-E4B-it-Q4_0.gguf".to_string(),
            size_mb: 4378,
            context_size: 8192,
            layer_count: 35,
            sampling: SamplingParams::gemma_instruct(vec!["<end_of_turn>".to_string()]),
            description: "Transcribes audio and writes summaries. Best accuracy of the built-in models. Needs ~5.2GB of downloads.".to_string(),
            mmproj: Some(Projector {
                file: "mmproj-gemma-4-E4B-it-BF16.gguf".to_string(),
                url: "https://huggingface.co/ggml-org/gemma-4-E4B-it-GGUF/resolve/main/mmproj-gemma-4-E4B-it-BF16.gguf".to_string(),
                size_mb: 946,
            }),
        },
        // Gemma 3 and Qwen 3.5 were dropped here: Gemma 4 supersedes both at the
        // same download sizes, and no newer family ships a small tier (Qwen 3.6
        // starts at 27B). Stored selections of the retired names fall back to
        // DEFAULT_SUMMARY_MODEL in api_get_model_config.
    ]
}

/// Get a specific model by name
pub fn get_model_by_name(name: &str) -> Option<ModelDef> {
    get_available_models().into_iter().find(|m| m.name == name)
}

/// Resolve model name to full file path in the models directory
pub fn get_model_path(app_data_dir: &PathBuf, model_name: &str) -> Result<PathBuf> {
    let model = get_model_by_name(model_name)
        .ok_or_else(|| anyhow!("Unknown model: {}", model_name))?;

    let models_dir = get_models_directory(app_data_dir);
    let model_path = models_dir.join(&model.gguf_file);

    Ok(model_path)
}

/// Get the models directory path for built-in AI
pub fn get_models_directory(app_data_dir: &PathBuf) -> PathBuf {
    app_data_dir.join("models").join("summary")
}

/// Resolve a model's projector path, if it has one.
pub fn get_mmproj_path(app_data_dir: &PathBuf, model_name: &str) -> Result<Option<PathBuf>> {
    let model = get_model_by_name(model_name)
        .ok_or_else(|| anyhow!("Unknown model: {}", model_name))?;

    Ok(model
        .mmproj
        .map(|p| get_models_directory(app_data_dir).join(p.file)))
}

/// Media marker mtmd substitutes with the encoded audio. Must appear exactly once
/// in a prompt that carries audio, or `mtmd_tokenize` rejects it.
pub const MEDIA_MARKER: &str = "<__media__>";

/// Instruction for audio transcription, kept deliberately narrow.
///
/// An LLM asked to transcribe can decline, editorialize, or narrate its reasoning.
/// Naming the output format is what stops it prefacing the transcript with "Sure,
/// here is the transcript".
const TRANSCRIBE_INSTRUCTION: &str = "Transcribe the speech in this audio verbatim. \
     Output only the spoken words, with no commentary, labels, or explanation. \
     If there is no intelligible speech, output nothing at all.";

/// Build the transcription prompt for an audio-capable model.
pub fn format_transcribe_prompt(template_name: &str) -> Result<String> {
    format_prompt(
        template_name,
        TRANSCRIBE_INSTRUCTION,
        &format!("{}\nTranscript:", MEDIA_MARKER),
    )
}

// ============================================================================
// Prompt Templates (Model-Specific Formatting)
// ============================================================================

/// Gemma chat template format. Gemma 4 kept Gemma 3's turn markers.
pub const GEMMA_TEMPLATE: &str = "\
<start_of_turn>user
{system_prompt}<end_of_turn>
<start_of_turn>user
{user_prompt}<end_of_turn>
<start_of_turn>model
";

fn escape_user_prompt_control_markers(user_prompt: &str) -> String {
    user_prompt
        .replace("<start_of_turn>", "< start_of_turn >")
        .replace("<end_of_turn>", "< end_of_turn >")
}

/// Format a prompt using the specified template
///
/// # Arguments
/// * `template_name` - Template identifier (e.g., "gemma3", "chatml", "llama3")
/// * `system_prompt` - System message (instructions for the model)
/// * `user_prompt` - User message (actual task/question)
///
/// # Returns
/// Formatted prompt string ready to send to llama-helper
pub fn format_prompt(
    template_name: &str,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String> {
    let template = match template_name {
        "gemma4" => GEMMA_TEMPLATE,
        _ => return Err(anyhow!("Unknown template: {}", template_name)),
    };

    let escaped_user_prompt = escape_user_prompt_control_markers(user_prompt);

    let formatted = template
        .replace("{system_prompt}", system_prompt)
        .replace("{user_prompt}", &escaped_user_prompt);

    Ok(formatted)
}

// ============================================================================
// Configuration Constants
// ============================================================================

/// Default max tokens for generation (increased for better summary quality)
pub const DEFAULT_MAX_TOKENS: i32 = 4096;

/// Idle timeout for sidecar (seconds) - can be overridden via LLAMA_IDLE_TIMEOUT env var
pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 300; // 5 minutes

/// Generation timeout (how long to wait for a response)
pub const GENERATION_TIMEOUT_SECS: u64 = 900; // 15 minutes

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcribe_prompt_keeps_exactly_one_media_marker() {
        // `format_prompt` escapes control markers in the user prompt. If
        // `<__media__>` ever joined that list, mtmd would see zero markers for one
        // bitmap and every segment would fail with BitmapCountMismatch.
        for model in get_available_models().iter().filter(|m| m.is_audio()) {
            let prompt = format_transcribe_prompt(&model.template).unwrap();
            assert_eq!(
                prompt.matches(MEDIA_MARKER).count(),
                1,
                "{} produced {:?}",
                model.name,
                prompt
            );
            assert!(prompt.contains("<start_of_turn>model"), "{}", model.name);
        }
    }

    #[test]
    fn audio_models_carry_a_projector_and_quote_both_downloads() {
        let audio: Vec<_> = get_available_models()
            .into_iter()
            .filter(|m| m.is_audio())
            .collect();
        assert!(!audio.is_empty(), "no audio-capable model is registered");

        for model in audio {
            let projector = model.mmproj.as_ref().expect("is_audio implies a projector");
            // BF16 on purpose: quantized projectors degrade transcripts.
            assert!(
                projector.file.contains("BF16"),
                "{} uses a quantized projector ({})",
                model.name,
                projector.file
            );
            assert!(projector.url.ends_with(&projector.file));
            // The quoted size has to cover both files or the progress bar lies.
            assert_eq!(
                model.total_size_mb(),
                model.size_mb + projector.size_mb,
                "{}",
                model.name
            );
        }
    }

    #[test]
    fn retired_families_are_no_longer_offered() {
        // Gemma 3 / Qwen 3.5 were removed. If one comes back it needs a template
        // arm too — `format_prompt` only knows "gemma4" now.
        for name in ["qwen3.5:2b", "qwen3.5:4b", "gemma3:4b", "gemma3:1b"] {
            assert!(get_model_by_name(name).is_none(), "{} is still offered", name);
        }
        for model in get_available_models() {
            assert!(
                format_prompt(&model.template, "sys", "user").is_ok(),
                "{} has no template arm",
                model.name
            );
        }
    }

    #[test]
    fn gemma_models_use_googles_recommended_sampling() {
        for model in get_available_models() {
            assert_eq!(
                model.sampling,
                SamplingParams::gemma_instruct(vec!["<end_of_turn>".to_string()]),
                "{}",
                model.name
            );
            assert_eq!(model.sampling.temperature, 1.0);
            assert_eq!(model.sampling.top_k, 64);
            assert_eq!(model.sampling.top_p, 0.95);
            assert_eq!(model.sampling.repeat_penalty, 1.0);
            assert_eq!(model.sampling.penalty_last_n, 0);
            assert!(model.download_url.starts_with("https://huggingface.co/"));
        }
    }

    #[test]
    fn gemma_template_escapes_user_supplied_control_markers() {
        let formatted = format_prompt(
            "gemma4",
            "system rules",
            "literal <start_of_turn> and <end_of_turn>",
        )
        .unwrap();

        assert!(formatted.contains("<start_of_turn>user\nsystem rules<end_of_turn>"));
        assert!(formatted.contains("literal < start_of_turn > and < end_of_turn >"));
        assert_eq!(formatted.matches("<start_of_turn>").count(), 3);
        assert_eq!(formatted.matches("<end_of_turn>").count(), 2);
    }

    #[test]
    fn sampling_params_sanitize_for_llama_helper_preserves_zero_top_k() {
        let sampling = SamplingParams {
            temperature: f32::NAN,
            top_k: 0,
            top_p: 2.0,
            presence_penalty: -0.5,
            frequency_penalty: f32::NAN,
            repeat_penalty: 0.0,
            penalty_last_n: -1,
            stop_tokens: vec!["stop".to_string()],
        };

        let sanitized = sampling.sanitize_for_llama_helper();

        assert_eq!(sanitized.temperature, 0.0);
        assert_eq!(sanitized.top_k, 0);
        assert_eq!(sanitized.top_p, 1.0);
        assert_eq!(sanitized.presence_penalty, 0.0);
        assert_eq!(sanitized.frequency_penalty, 0.0);
        assert_eq!(sanitized.repeat_penalty, 1.0);
        assert_eq!(sanitized.penalty_last_n, 0);
        assert_eq!(sanitized.stop_tokens, vec!["stop".to_string()]);
    }

    #[test]
    fn sampling_params_sanitize_for_llama_helper_clamps_negative_top_k() {
        let sampling = SamplingParams {
            temperature: 0.7,
            top_k: -5,
            top_p: 0.8,
            presence_penalty: 0.3,
            frequency_penalty: 0.0,
            repeat_penalty: 1.05,
            penalty_last_n: 256,
            stop_tokens: vec!["stop".to_string()],
        };

        let sanitized = sampling.sanitize_for_llama_helper();

        assert_eq!(sanitized.top_k, 0);
        assert_eq!(sanitized.temperature, 0.7);
        assert_eq!(sanitized.top_p, 0.8);
        assert_eq!(sanitized.presence_penalty, 0.3);
        assert_eq!(sanitized.repeat_penalty, 1.05);
        assert_eq!(sanitized.penalty_last_n, 256);
    }

    #[test]
    fn sampling_params_sanitize_for_llama_helper_keeps_positive_top_k() {
        let sampling = SamplingParams::gemma_instruct(vec!["stop".to_string()]);

        let sanitized = sampling.sanitize_for_llama_helper();

        assert_eq!(sanitized.top_k, 64);
    }
}
