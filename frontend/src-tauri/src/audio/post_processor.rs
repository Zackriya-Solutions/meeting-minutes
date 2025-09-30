use std::sync::Arc;
use tokio::sync::mpsc;
use anyhow::Result;
use log::{info, warn, error};

/// Post-processing request for transcript text
#[derive(Debug, Clone)]
pub struct PostProcessRequest {
    pub sequence_id: u32,
    pub raw_text: String,
    pub is_partial: bool,
    pub timestamp: String,
}

/// Post-processing response with refined text
#[derive(Debug, Clone)]
pub struct PostProcessResponse {
    pub sequence_id: u32,
    pub processed_text: String,
    pub confidence: f32,
    pub is_partial: bool,
    pub timestamp: String,
    pub processing_time_ms: u64,
}

/// Background post-processing pipeline for transcript text
pub struct PostProcessor {
    request_sender: mpsc::UnboundedSender<PostProcessRequest>,
    response_receiver: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<PostProcessResponse>>>,
    _handle: tokio::task::JoinHandle<()>,
}

impl PostProcessor {
    /// Create a new post-processor with background processing
    pub fn new() -> Self {
        let (request_sender, mut request_receiver) = mpsc::unbounded_channel();
        let (response_sender, response_receiver) = mpsc::unbounded_channel();

        let handle = tokio::spawn(async move {
            info!("Background post-processor started");

            while let Some(request) = request_receiver.recv().await {
                let start_time = std::time::Instant::now();

                match Self::process_text(&request).await {
                    Ok(processed_text) => {
                        let processing_time = start_time.elapsed().as_millis() as u64;

                        let response = PostProcessResponse {
                            sequence_id: request.sequence_id,
                            processed_text,
                            confidence: if request.is_partial { 0.8 } else { 0.95 }, // Processed text has higher confidence
                            is_partial: request.is_partial,
                            timestamp: request.timestamp,
                            processing_time_ms: processing_time,
                        };

                        if let Err(e) = response_sender.send(response) {
                            error!("Failed to send post-processing response: {}", e);
                            break;
                        }

                        if processing_time > 100 {
                            warn!("Slow post-processing for sequence {}: {}ms", request.sequence_id, processing_time);
                        }
                    }
                    Err(e) => {
                        warn!("Post-processing failed for sequence {}: {}", request.sequence_id, e);
                        // Send original text as fallback
                        let response = PostProcessResponse {
                            sequence_id: request.sequence_id,
                            processed_text: request.raw_text.clone(),
                            confidence: 0.5, // Lower confidence for failed processing
                            is_partial: request.is_partial,
                            timestamp: request.timestamp,
                            processing_time_ms: start_time.elapsed().as_millis() as u64,
                        };

                        if let Err(e) = response_sender.send(response) {
                            error!("Failed to send fallback response: {}", e);
                            break;
                        }
                    }
                }
            }

            info!("Background post-processor stopped");
        });

        Self {
            request_sender,
            response_receiver: Arc::new(tokio::sync::Mutex::new(response_receiver)),
            _handle: handle,
        }
    }

    /// Submit text for background post-processing
    pub fn process_async(&self, request: PostProcessRequest) -> Result<()> {
        self.request_sender
            .send(request)
            .map_err(|e| anyhow::anyhow!("Failed to submit post-processing request: {}", e))
    }

    /// Try to receive processed results (non-blocking)
    pub async fn try_recv(&self) -> Option<PostProcessResponse> {
        let mut receiver = self.response_receiver.lock().await;
        receiver.try_recv().ok()
    }

    /// Wait for the next processed result
    pub async fn recv(&self) -> Option<PostProcessResponse> {
        let mut receiver = self.response_receiver.lock().await;
        receiver.recv().await
    }

    /// Process text synchronously (for testing or direct use)
    async fn process_text(request: &PostProcessRequest) -> Result<String> {
        let text = &request.raw_text;

        // Skip processing for empty or very short text
        if text.trim().len() < 3 {
            return Ok(text.clone());
        }

        // Step 1: Clean repetitive text (most expensive operation)
        let deduplicated = Self::clean_repetitive_text(text);

        // Step 2: Remove common transcription artifacts
        let cleaned = Self::remove_artifacts(&deduplicated);

        // Step 3: Normalize whitespace and punctuation
        let normalized = Self::normalize_text(&cleaned);

        // Step 4: Apply contextual improvements (if not partial)
        let final_text = if !request.is_partial {
            Self::apply_contextual_improvements(&normalized)
        } else {
            normalized
        };

        Ok(final_text)
    }

    /// Clean repetitive text patterns (same as whisper_engine but moved to background)
    fn clean_repetitive_text(text: &str) -> String {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.len() < 4 {
            return text.to_string();
        }

        let mut result = Vec::new();
        let mut i = 0;

        while i < words.len() {
            let current_word = words[i];

            // Check for immediate repetitions (same word repeated)
            if i + 1 < words.len() && words[i + 1] == current_word {
                result.push(current_word);
                // Skip repeated instances
                while i + 1 < words.len() && words[i + 1] == current_word {
                    i += 1;
                }
            }
            // Check for phrase repetitions
            else if i + 3 < words.len() {
                let phrase = &words[i..i+2];
                let next_phrase = &words[i+2..i+4];

                if phrase == next_phrase {
                    result.extend_from_slice(phrase);
                    i += 4; // Skip both phrases

                    // Skip additional repetitions of the same phrase
                    while i + 1 < words.len() && i + 1 < words.len() - 1 {
                        let check_phrase = &words[i..std::cmp::min(i+2, words.len())];
                        if check_phrase == phrase && check_phrase.len() == 2 {
                            i += 2;
                        } else {
                            break;
                        }
                    }
                    continue;
                }
                result.push(current_word);
            } else {
                result.push(current_word);
            }
            i += 1;
        }

        result.join(" ")
    }

    /// Remove common transcription artifacts using simple string matching
    fn remove_artifacts(text: &str) -> String {
        let mut words: Vec<String> = text.split_whitespace()
            .map(|w| w.to_string())
            .collect();

        // Remove common filler words and sounds
        let fillers = [
            "uh", "um", "er", "ah", "oh", "hm", "hmm",
            "uhh", "umm", "err", "ahh", "ohh",
        ];

        words.retain(|word| {
            let clean_word_temp = word.to_lowercase();
            let clean_word = clean_word_temp.trim_matches(|c: char| !c.is_alphabetic());
            !fillers.contains(&clean_word) || clean_word.len() > 3
        });

        words.join(" ")
    }

    /// Normalize text formatting
    fn normalize_text(text: &str) -> String {
        let mut normalized = text.trim().to_string();

        // Fix spacing around punctuation
        normalized = normalized.replace(" .", ".");
        normalized = normalized.replace(" ,", ",");
        normalized = normalized.replace(" ?", "?");
        normalized = normalized.replace(" !", "!");

        // Ensure single space after sentence endings
        normalized = normalized.replace(".  ", ". ");
        normalized = normalized.replace("?  ", "? ");
        normalized = normalized.replace("!  ", "! ");

        // Capitalize first letter of sentences
        if let Some(first_char) = normalized.chars().next() {
            if first_char.is_lowercase() {
                normalized = first_char.to_uppercase().collect::<String>() + &normalized[1..];
            }
        }

        normalized
    }

    /// Apply contextual improvements for final transcripts
    fn apply_contextual_improvements(text: &str) -> String {
        let mut improved = text.to_string();

        // Common word corrections (could be expanded with a dictionary)
        let corrections = [
            ("cant", "can't"),
            ("wont", "won't"),
            ("dont", "don't"),
            ("doesnt", "doesn't"),
            ("didnt", "didn't"),
            ("wouldnt", "wouldn't"),
            ("couldnt", "couldn't"),
            ("shouldnt", "shouldn't"),
            ("isnt", "isn't"),
            ("arent", "aren't"),
            ("wasnt", "wasn't"),
            ("werent", "weren't"),
            ("hasnt", "hasn't"),
            ("havent", "haven't"),
            ("hadnt", "hadn't"),
        ];

        for (incorrect, correct) in &corrections {
            improved = improved.replace(incorrect, correct);
        }

        improved
    }
}

impl Default for PostProcessor {
    fn default() -> Self {
        Self::new()
    }
}