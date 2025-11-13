use crate::summary::llm_client::{generate_summary, LLMProvider};
use crate::summary::prompts;
use crate::summary::templates;
use regex::Regex;
use reqwest::Client;
use tracing::{error, info};

/// Rough token count estimation (4 characters ≈ 1 token)
pub fn rough_token_count(s: &str) -> usize {
    (s.chars().count() as f64 / 4.0).ceil() as usize
}

/// Chunks text into overlapping segments based on token count
///
/// # Detailed Documentation - Date: 13/11/2025 - Author: Luiz
///
/// This method implements a sophisticated chunking algorithm for processing long transcripts
/// that exceed the LLM model's context limit. The algorithm ensures:
///
/// 1. **Respects Token Limits**: Each chunk does not exceed the specified maximum size
/// 2. **Maintains Context with Overlap**: Consecutive chunks share tokens to preserve context
/// 3. **Preserves Word Integrity**: Never cuts words in the middle (word-boundary detection)
/// 4. **Optimizes Performance**: Uses fast token estimation (4 chars ≈ 1 token)
///
/// # Algorithm Flow:
///
/// 1. Converts token sizes to characters (multiplying by 4)
/// 2. If text is smaller than chunk_size, returns as single chunk
/// 3. Calculates step_size = chunk_size - overlap (non-overlapping portion)
/// 4. Iterates through string with sliding window:
///    - Defines end_pos = current_pos + chunk_size
///    - Searches backward for whitespace to avoid cutting words
///    - Extracts chunk and adds to vector
///    - Advances current_pos by step_size
/// 5. Returns all created chunks
///
/// # Practical Example:
///
/// ```text
/// Text: "The quick brown fox jumps over the lazy dog and runs away"
/// chunk_size_tokens: 10 (40 chars)
/// overlap_tokens: 2 (8 chars)
/// step_size: 8 tokens (32 chars)
///
/// Chunk 1: "The quick brown fox jumps over the"  [0..35]
/// Chunk 2:      "fox jumps over the lazy dog and"  [32..64]
/// Chunk 3:              "the lazy dog and runs away" [56..82]
/// ```
///
/// # Arguments
/// * `text` - The text to chunk
/// * `chunk_size_tokens` - Maximum tokens per chunk
/// * `overlap_tokens` - Number of overlapping tokens between chunks
///
/// # Returns
/// Vector of text chunks with smart word-boundary splitting
pub fn chunk_text(text: &str, chunk_size_tokens: usize, overlap_tokens: usize) -> Vec<String> {
    info!(
        "Chunking text with token-based chunk_size: {} and overlap: {}",
        chunk_size_tokens, overlap_tokens
    );

    if text.is_empty() || chunk_size_tokens == 0 {
        return vec![];
    }

    // Convert token-based sizes to character-based sizes (4 chars ≈ 1 token)
    let chunk_size_chars = chunk_size_tokens * 4;
    let overlap_chars = overlap_tokens * 4;

    let chars: Vec<char> = text.chars().collect();
    let total_chars = chars.len();

    if total_chars <= chunk_size_chars {
        info!("Text is shorter than chunk size, returning as a single chunk.");
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current_pos = 0;
    // Step is the size of the non-overlapping part of the window
    let step = chunk_size_chars.saturating_sub(overlap_chars).max(1);

    while current_pos < total_chars {
        let mut end_pos = std::cmp::min(current_pos + chunk_size_chars, total_chars);

        // Try to find a whitespace boundary to avoid splitting words
        if end_pos < total_chars {
            let mut boundary = end_pos;
            while boundary > current_pos && !chars[boundary].is_whitespace() {
                boundary -= 1;
            }
            if boundary > current_pos {
                end_pos = boundary;
            }
        }

        let chunk: String = chars[current_pos..end_pos].iter().collect();
        chunks.push(chunk);

        if end_pos == total_chars {
            break;
        }

        current_pos += step;
    }

    info!("Created {} chunks from text", chunks.len());
    chunks
}

/// Cleans markdown output from LLM by removing thinking tags and code fences
///
/// # Detailed Documentation - Date: 13/11/2025 - Author: Luiz
///
/// This method performs essential post-processing on raw LLM output to extract
/// only pure, usable markdown content. Many modern LLMs (especially Claude)
/// include "thinking" tags and wrap output in code fences (```markdown),
/// which need to be removed.
///
/// # Cleaning Steps:
///
/// 1. **Thinking Tags Removal**:
///    - Removes `<think>...</think>` or `<thinking>...</thinking>` blocks
///    - Uses regex with `(?s)` flag for multi-line matching
///    - These blocks contain internal LLM reasoning that shouldn't appear in output
///
/// 2. **Code Fences Removal**:
///    - Detects if output is wrapped in ```markdown\n or ```\n
///    - Extracts only content between fences
///    - Supports multiple fence formats (with or without language specifier)
///
/// 3. **Trimming**:
///    - Removes whitespace at beginning/end after each step
///    - Ensures clean, ready-to-use output
///
/// # Example:
///
/// Input:
/// ```markdown
/// <thinking>I'll create a structured summary...</thinking>
///
/// ```markdown
/// # Planning Meeting
///
/// **Summary**: Discussion about features...
/// ```
/// ```
///
/// Output:
/// ```text
/// # Planning Meeting
///
/// **Summary**: Discussion about features...
/// ```
///
/// # Arguments
/// * `markdown` - Raw markdown output from LLM
///
/// # Returns
/// Cleaned markdown string
pub fn clean_llm_markdown_output(markdown: &str) -> String {
    // Remove <think>...</think> or <thinking>...</thinking> blocks
    let re = Regex::new(r"(?s)<think(?:ing)?>.*?</think(?:ing)?>").unwrap();
    let without_thinking = re.replace_all(markdown, "");

    let trimmed = without_thinking.trim();

    // List of possible language identifiers for code blocks
    const PREFIXES: &[&str] = &["```markdown\n", "```\n"];
    const SUFFIX: &str = "```";

    for prefix in PREFIXES {
        if trimmed.starts_with(prefix) && trimmed.ends_with(SUFFIX) {
            // Extract content between the fences
            let content = &trimmed[prefix.len()..trimmed.len() - SUFFIX.len()];
            return content.trim().to_string();
        }
    }

    // If no fences found, return the trimmed string
    trimmed.to_string()
}

/// Extracts meeting name from the first heading in markdown
///
/// # Arguments
/// * `markdown` - Markdown content
///
/// # Returns
/// Meeting name if found, None otherwise
pub fn extract_meeting_name_from_markdown(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .find(|line| line.starts_with("# "))
        .map(|line| line.trim_start_matches("# ").trim().to_string())
}

/// Generates a complete meeting summary with conditional chunking strategy
///
/// # Detailed Documentation - Date: 13/11/2025 - Author: Luiz
///
/// This is the central and most important method of the summary generation system.
/// It implements an intelligent summarization strategy that automatically adapts
/// to the transcript size and LLM provider being used.
///
/// # Decision Strategy - Chunking vs Single-Pass:
///
/// **Single-Pass** (used when):
/// - Provider is cloud-based (OpenAI, Claude, Groq, OpenRouter) - unlimited context
/// - OR transcript is short (< token_threshold, default 4000 tokens)
/// - Advantage: Faster, single LLM call, complete context
///
/// **Multi-Level Chunking** (used when):
/// - Provider is Ollama (limited context based on model)
/// - AND transcript is long (>= token_threshold)
/// - Flow in 3 stages:
///   1. Divide transcript into chunks with overlap (chunk_text)
///   2. Generate partial summary for each chunk (generate_summary for each chunk)
///   3. Combine partial summaries into coherent narrative (generate_summary for combination)
///
/// # Final Processing (both strategies):
///
/// 1. **Load Template**: Fetch template JSON (fallback: custom → bundled → built-in)
/// 2. **Generate Structure**: Create markdown skeleton and section instructions
/// 3. **Build Prompts**: System prompt with instructions + User prompt with transcript
/// 4. **Call LLM**: Send to provider with formatted prompts
/// 5. **Clean Output**: Remove thinking tags and code fences
/// 6. **Return**: Final markdown + chunk count processed
///
/// # Multi-Level Visual Flow:
///
/// ```text
/// Long Transcript (15,000 tokens)
///         ↓
///   [chunk_text]
///         ↓
///    ┌─────────┬─────────┬─────────┬─────────┐
///    │ Chunk 1 │ Chunk 2 │ Chunk 3 │ Chunk 4 │
///    │ (3700t) │ (3700t) │ (3700t) │ (3700t) │
///    └────┬────┴────┬────┴────┬────┴────┬────┘
///         │         │         │         │
///    [Summary 1] [Summary 2] [Summary 3] [Summary 4]
///         └─────────┴─────────┴─────────┘
///                     │
///           [Combine Summaries]
///                     ↓
///            Unified Summary
///                     ↓
///         [Apply Final Template]
///                     ↓
///           Markdown Report
/// ```
///
/// # Arguments
/// * `client` - Reqwest HTTP client
/// * `provider` - LLM provider to use
/// * `model_name` - Specific model name
/// * `api_key` - API key for the provider
/// * `text` - Full transcript text to summarize
/// * `custom_prompt` - Optional user-provided context
/// * `template_id` - Template identifier (e.g., "daily_standup", "standard_meeting")
/// * `token_threshold` - Token limit for single-pass processing (default 4000)
/// * `ollama_endpoint` - Optional custom Ollama endpoint
/// * `language` - Language for prompts: 'pt' (Portuguese) or 'en' (English) - Added 13/11/2025 by Luiz
///
/// # Returns
/// Tuple of (final_summary_markdown, number_of_chunks_processed)
pub async fn generate_meeting_summary(
    client: &Client,
    provider: &LLMProvider,
    model_name: &str,
    api_key: &str,
    text: &str,
    custom_prompt: &str,
    template_id: &str,
    token_threshold: usize,
    ollama_endpoint: Option<&str>,
    language: &str,
) -> Result<(String, i64), String> {
    info!(
        "Starting summary generation with provider: {:?}, model: {}",
        provider, model_name
    );

    let total_tokens = rough_token_count(text);
    info!("Transcript length: {} tokens", total_tokens);

    let content_to_summarize: String;
    let successful_chunk_count: i64;

    // =================================================================================
    // STRATEGY DECISION - Date: 13/11/2025 - Author: Luiz
    // =================================================================================
    // This is the critical decision that determines which summarization strategy to use:
    //
    // CONDITION: provider != Ollama OR total_tokens < threshold
    //
    // WHEN SINGLE-PASS (condition is true):
    // - Cloud providers (OpenAI, Claude, Groq, OpenRouter) have ~100k+ tokens context
    // - Short transcripts (<4000 tokens default) fit in any context
    // - BENEFIT: Faster (1 call), better quality (complete context)
    //
    // WHEN MULTI-LEVEL (condition is false):
    // - Ollama with local models have limited context (e.g., llama3.2 = 2048 tokens)
    // - Long transcripts need to be divided to fit in context
    // - BENEFIT: Works with limited models, processes very long meetings
    // =================================================================================
    if provider != &LLMProvider::Ollama || total_tokens < token_threshold {
        info!(
            "Using single-pass summarization (tokens: {}, threshold: {})",
            total_tokens, token_threshold
        );
        content_to_summarize = text.to_string();
        successful_chunk_count = 1;
    } else {
        info!(
            "Using multi-level summarization (tokens: {} exceeds threshold: {})",
            total_tokens, token_threshold
        );

        // Reserve 300 tokens for prompt overhead
        let chunks = chunk_text(text, token_threshold - 300, 100);
        let num_chunks = chunks.len();
        info!("Split transcript into {} chunks", num_chunks);

        let mut chunk_summaries = Vec::new();

        // Get prompts in the appropriate language
        // Date: 13/11/2025 - Author: Luiz
        let system_prompt_chunk = prompts::get_chunk_system_prompt(language);
        let user_prompt_template_chunk = prompts::get_chunk_user_prompt_template(language);

        for (i, chunk) in chunks.iter().enumerate() {
            info!("⏲️ Processing chunk {}/{}", i + 1, num_chunks);
            let user_prompt_chunk = user_prompt_template_chunk.replace("{}", chunk.as_str());

            match generate_summary(
                client,
                provider,
                model_name,
                api_key,
                system_prompt_chunk,
                &user_prompt_chunk,
                ollama_endpoint,
            )
            .await
            {
                Ok(summary) => {
                    chunk_summaries.push(summary);
                    info!("✓ Chunk {}/{} processed successfully", i + 1, num_chunks);
                }
                Err(e) => {
                    error!("⚠️ Failed processing chunk {}/{}: {}", i + 1, num_chunks, e);
                }
            }
        }

        if chunk_summaries.is_empty() {
            return Err(
                "Multi-level summarization failed: No chunks were processed successfully."
                    .to_string(),
            );
        }

        successful_chunk_count = chunk_summaries.len() as i64;
        info!(
            "Successfully processed {} out of {} chunks",
            successful_chunk_count, num_chunks
        );

        // Combine chunk summaries if multiple chunks
        content_to_summarize = if chunk_summaries.len() > 1 {
            info!(
                "Combining {} chunk summaries into cohesive summary",
                chunk_summaries.len()
            );
            let combined_text = chunk_summaries.join("\n---\n");

            // Get combine prompts in the appropriate language
            // Date: 13/11/2025 - Author: Luiz
            let system_prompt_combine = prompts::get_combine_system_prompt(language);
            let user_prompt_combine_template = prompts::get_combine_user_prompt_template(language);

            let user_prompt_combine = user_prompt_combine_template.replace("{}", &combined_text);
            generate_summary(
                client,
                provider,
                model_name,
                api_key,
                system_prompt_combine,
                &user_prompt_combine,
                ollama_endpoint,
            )
            .await?
        } else {
            chunk_summaries.remove(0)
        };
    }

    info!("Generating final markdown report with template: {}", template_id);

    // Load the template using the provided template_id
    let template = templates::get_template(template_id)
        .map_err(|e| format!("Failed to load template '{}': {}", template_id, e))?;

    // Generate markdown structure and section instructions using template methods
    let clean_template_markdown = template.to_markdown_structure();
    let section_instructions = template.to_section_instructions();

    // Get final prompt template in the appropriate language and format it
    // Date: 13/11/2025 - Author: Luiz
    let final_system_prompt = format!(
        prompts::get_final_system_prompt_template(language),
        section_instructions, clean_template_markdown
    );

    let mut final_user_prompt = format!(
        r#"
<transcript_chunks>
{}
</transcript_chunks>
"#,
        content_to_summarize
    );

    if !custom_prompt.is_empty() {
        final_user_prompt.push_str("\n\nUser Provided Context:\n\n<user_context>\n");
        final_user_prompt.push_str(custom_prompt);
        final_user_prompt.push_str("\n</user_context>");
    }

    let raw_markdown = generate_summary(
        client,
        provider,
        model_name,
        api_key,
        &final_system_prompt,
        &final_user_prompt,
        ollama_endpoint,
    )
    .await?;

    // Clean the output
    let final_markdown = clean_llm_markdown_output(&raw_markdown);

    info!("Summary generation completed successfully");
    Ok((final_markdown, successful_chunk_count))
}
