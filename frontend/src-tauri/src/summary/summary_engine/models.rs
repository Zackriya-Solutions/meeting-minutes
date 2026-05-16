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

    /// Stop tokens - generation stops when any of these appear in output
    pub stop_tokens: Vec<String>,
}

impl SamplingParams {
    /// Safest extraction preset. llama-helper treats temperature <= 0.0 as greedy decoding.
    pub fn greedy_strict(stop_tokens: Vec<String>) -> Self {
        Self {
            temperature: 0.0,
            top_k: 1,
            top_p: 1.0,
            stop_tokens,
        }
    }

    /// Restrained near-greedy preset for fuller but still conservative output.
    pub fn tight_structured(stop_tokens: Vec<String>) -> Self {
        Self {
            temperature: 0.1,
            top_k: 20,
            top_p: 0.88,
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
        let top_k = self.top_k.max(1);
        let top_p = if self.top_p.is_finite() && self.top_p > 0.0 && self.top_p <= 1.0 {
            self.top_p
        } else {
            1.0
        };

        Self {
            temperature,
            top_k,
            top_p,
            stop_tokens: self.stop_tokens.clone(),
        }
    }
}

/// Definition of a built-in AI model with all metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDef {
    /// Model name in format "family:variant" (e.g., "gemma3:1b")
    /// This is what's stored in database as model field when provider="builtin-ai"
    pub name: String,

    /// Display name for UI (e.g., "Gemma 3 1B (Fast)")
    pub display_name: String,

    /// GGUF filename on disk (e.g., "gemma-3-1b-it-q4_0.gguf")
    pub gguf_file: String,

    /// Template name for prompt formatting (e.g., "gemma3")
    pub template: String,

    /// Download URL (HuggingFace or other source)
    pub download_url: String,

    /// File size in MB
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
}

/// Get all available built-in AI models
/// Add new models here - the system will automatically detect and manage them
pub fn get_available_models() -> Vec<ModelDef> {
    vec![
        // Qwen 3.5 2B - Balanced tier
        ModelDef {
            name: "qwen3.5:2b".to_string(),
            display_name: "Qwen 3.5 2B (Balanced)".to_string(),
            gguf_file: "Qwen3.5-2B-Q4_K_M.gguf".to_string(),
            template: "qwen3.5_nonthinking".to_string(),
            download_url: "https://huggingface.co/unsloth/Qwen3.5-2B-GGUF/resolve/main/Qwen3.5-2B-Q4_K_M.gguf".to_string(),
            size_mb: 1270,
            context_size: 32768,
            layer_count: 24,
            sampling: SamplingParams::greedy_strict(vec!["<|im_end|>".to_string()]),
            description: "Balanced Qwen 3.5 model for built-in summaries. Higher quality with modest local requirements.".to_string(),
        },
        // Qwen 3.5 4B - High quality tier
        ModelDef {
            name: "qwen3.5:4b".to_string(),
            display_name: "Qwen 3.5 4B (High Quality)".to_string(),
            gguf_file: "Qwen3.5-4B-Q4_K_M.gguf".to_string(),
            template: "qwen3.5_nonthinking".to_string(),
            download_url: "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf".to_string(),
            size_mb: 2614,
            context_size: 32768,
            layer_count: 32,
            sampling: SamplingParams::greedy_strict(vec!["<|im_end|>".to_string()]),
            description: "High-quality Qwen 3.5 model for built-in summaries. Best local Qwen option in the current lineup.".to_string(),
        },
        // Gemma 3 4B - Legacy alternative retained for users who prefer Gemma output.
        ModelDef {
            name: "gemma3:4b".to_string(),
            display_name: "Gemma 3 4B (Balanced)".to_string(),
            gguf_file: "gemma-3-4b-it-Q4_K_M.gguf".to_string(),
            template: "gemma3".to_string(),
            download_url: "https://huggingface.co/bartowski/google_gemma-3-4b-it-GGUF/resolve/main/google_gemma-3-4b-it-Q4_K_M.gguf".to_string(),
            size_mb: 2374,
            context_size: 32768,
            layer_count: 35,
            sampling: SamplingParams::tight_structured(vec!["<end_of_turn>".to_string()]),
            description: "Balanced model. Great quality/speed trade-off. Requires ~3.5GB RAM.".to_string(),
        },
        // Gemma 3 1B - Visible legacy tier retained for already-shipped users.
        ModelDef {
            name: "gemma3:1b".to_string(),
            display_name: "Gemma 3 1B (Fast)".to_string(),
            gguf_file: "gemma-3-1b-it-Q8_0.gguf".to_string(),
            template: "gemma3".to_string(),
            download_url: "https://huggingface.co/bartowski/google_gemma-3-1b-it-GGUF/resolve/main/google_gemma-3-1b-it-Q8_0.gguf".to_string(),
            size_mb: 1019,
            context_size: 32768,
            layer_count: 26,
            sampling: SamplingParams::tight_structured(vec!["<end_of_turn>".to_string()]),
            description: "Fastest model. Runs on any hardware with ~1GB RAM. Good for quick summaries.".to_string(),
        },
    ]
}

/// Get a specific model by name
pub fn get_model_by_name(name: &str) -> Option<ModelDef> {
    get_available_models().into_iter().find(|m| m.name == name)
}

/// Get the default model (first in list)
pub fn get_default_model() -> ModelDef {
    get_available_models()
        .into_iter()
        .next()
        .expect("At least one model must be defined")
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

// ============================================================================
// Prompt Templates (Model-Specific Formatting)
// ============================================================================

/// Gemma 3 chat template format
pub const GEMMA3_TEMPLATE: &str = "\
<start_of_turn>user
{system_prompt}<end_of_turn>
<start_of_turn>user
{user_prompt}<end_of_turn>
<start_of_turn>model
";

/// Qwen 3.5 non-thinking chat template format.
/// This starts the assistant turn with an empty think block so generation begins
/// in direct-response mode for summaries.
pub const QWEN35_NONTHINKING_TEMPLATE: &str = "\
<|im_start|>system
{system_prompt}<|im_end|>
<|im_start|>user
{user_prompt}<|im_end|>
<|im_start|>assistant
<think>

</think>

";

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
        "gemma3" => GEMMA3_TEMPLATE,
        "qwen3.5_nonthinking" => QWEN35_NONTHINKING_TEMPLATE,
        _ => return Err(anyhow!("Unknown template: {}", template_name)),
    };

    let formatted = template
        .replace("{system_prompt}", system_prompt)
        .replace("{user_prompt}", user_prompt);

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
    fn qwen35_models_are_registered_with_expected_metadata() {
        let qwen_2b = get_model_by_name("qwen3.5:2b").expect("qwen 2b model should exist");
        assert_eq!(qwen_2b.display_name, "Qwen 3.5 2B (Balanced)");
        assert_eq!(qwen_2b.gguf_file, "Qwen3.5-2B-Q4_K_M.gguf");
        assert_eq!(qwen_2b.template, "qwen3.5_nonthinking");
        assert_eq!(
            qwen_2b.download_url,
            "https://huggingface.co/unsloth/Qwen3.5-2B-GGUF/resolve/main/Qwen3.5-2B-Q4_K_M.gguf"
        );
        assert_eq!(qwen_2b.size_mb, 1270);
        assert_eq!(qwen_2b.context_size, 32768);
        assert_eq!(qwen_2b.layer_count, 24);
        assert_eq!(qwen_2b.sampling, SamplingParams::greedy_strict(vec!["<|im_end|>".to_string()]));

        let qwen_4b = get_model_by_name("qwen3.5:4b").expect("qwen 4b model should exist");
        assert_eq!(qwen_4b.display_name, "Qwen 3.5 4B (High Quality)");
        assert_eq!(qwen_4b.gguf_file, "Qwen3.5-4B-Q4_K_M.gguf");
        assert_eq!(qwen_4b.template, "qwen3.5_nonthinking");
        assert_eq!(
            qwen_4b.download_url,
            "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf"
        );
        assert_eq!(qwen_4b.size_mb, 2614);
        assert_eq!(qwen_4b.context_size, 32768);
        assert_eq!(qwen_4b.layer_count, 32);
        assert_eq!(qwen_4b.sampling, SamplingParams::greedy_strict(vec!["<|im_end|>".to_string()]));
    }

    #[test]
    fn gemma_models_use_huggingface_urls_and_tight_structured_sampling() {
        let gemma_1b = get_model_by_name("gemma3:1b").expect("gemma 1b model should exist");
        assert_eq!(gemma_1b.gguf_file, "gemma-3-1b-it-Q8_0.gguf");
        assert_eq!(
            gemma_1b.download_url,
            "https://huggingface.co/bartowski/google_gemma-3-1b-it-GGUF/resolve/main/google_gemma-3-1b-it-Q8_0.gguf"
        );
        assert_eq!(gemma_1b.sampling, SamplingParams::tight_structured(vec!["<end_of_turn>".to_string()]));

        let gemma_4b = get_model_by_name("gemma3:4b").expect("gemma 4b model should exist");
        assert_eq!(
            gemma_4b.download_url,
            "https://huggingface.co/bartowski/google_gemma-3-4b-it-GGUF/resolve/main/google_gemma-3-4b-it-Q4_K_M.gguf"
        );
        assert_eq!(gemma_4b.sampling, SamplingParams::tight_structured(vec!["<end_of_turn>".to_string()]));
    }

    #[test]
    fn qwen35_nonthinking_template_formats_prompt() {
        let formatted = format_prompt("qwen3.5_nonthinking", "system rules", "summarize this").unwrap();

        assert!(formatted.contains("<|im_start|>system\nsystem rules<|im_end|>"));
        assert!(formatted.contains("<|im_start|>user\nsummarize this<|im_end|>"));
        assert!(formatted.ends_with("<think>\n\n</think>\n\n"));
    }

    #[test]
    fn sampling_params_sanitize_for_llama_helper_clamps_invalid_values() {
        let sampling = SamplingParams {
            temperature: f32::NAN,
            top_k: 0,
            top_p: 2.0,
            stop_tokens: vec!["stop".to_string()],
        };

        let sanitized = sampling.sanitize_for_llama_helper();

        assert_eq!(sanitized.temperature, 0.0);
        assert_eq!(sanitized.top_k, 1);
        assert_eq!(sanitized.top_p, 1.0);
        assert_eq!(sanitized.stop_tokens, vec!["stop".to_string()]);
    }
}
