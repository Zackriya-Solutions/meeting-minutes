use super::model::TimestampedResult;

pub const PARAKEET_CTC_ZH_CN_MODEL: &str = "parakeet-ctc-0.6b-zh-cn-int8";
pub const PARAKEET_CTC_ZH_CN_MAX_SAMPLES: usize = 15 * 16_000;
pub const PARAKEET_CTC_ZH_CN_BLANK_ID: usize = 7_000;
const CTC_FRAME_SECONDS: f32 = 0.08;

pub fn decode_ctc_logits(
    logits: &[f32],
    time_steps: usize,
    vocab_size_with_blank: usize,
    valid_time_steps: usize,
    vocab: &[String],
    blank_id: usize,
) -> Result<TimestampedResult, String> {
    if vocab_size_with_blank == 0 || logits.len() != time_steps * vocab_size_with_blank {
        return Err(format!(
            "Invalid CTC logits shape: {} values for {}x{}",
            logits.len(),
            time_steps,
            vocab_size_with_blank
        ));
    }
    if blank_id >= vocab_size_with_blank {
        return Err(format!("Invalid CTC blank token id: {}", blank_id));
    }

    let mut previous = None;
    let mut token_ids = Vec::new();
    let mut timestamps = Vec::new();

    for (time_index, frame) in logits
        .chunks_exact(vocab_size_with_blank)
        .take(valid_time_steps.min(time_steps))
        .enumerate()
    {
        let token_id = frame
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| {
                left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(index, _)| index)
            .unwrap_or(blank_id);

        if token_id != blank_id && previous != Some(token_id) {
            token_ids.push(token_id);
            timestamps.push(time_index as f32 * CTC_FRAME_SECONDS);
        }
        previous = Some(token_id);
    }

    let tokens: Vec<String> = token_ids
        .into_iter()
        .filter_map(|token_id| vocab.get(token_id).cloned())
        .collect();
    let text = normalize_ctc_text(&tokens.join("").replace('\u{2581}', " "));

    Ok(TimestampedResult {
        text,
        timestamps,
        tokens,
    })
}

pub fn join_chunk_text(parts: &[String]) -> String {
    normalize_ctc_text(&parts.join(" "))
}

fn normalize_ctc_text(text: &str) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = compact.chars().collect();
    let mut result = String::with_capacity(compact.len());

    for (index, character) in chars.iter().copied().enumerate() {
        if character == ' ' {
            let previous = result.chars().next_back();
            let next = chars.get(index + 1).copied();
            let between_cjk = previous.is_some_and(is_cjk) && next.is_some_and(is_cjk);
            let after_punctuation = previous.is_some_and(is_cjk_punctuation);
            let before_punctuation = next.is_some_and(is_cjk_punctuation);
            if between_cjk || after_punctuation || before_punctuation {
                continue;
            }
        }
        result.push(character);
    }

    result.trim().to_string()
}

fn is_cjk(character: char) -> bool {
    matches!(
        character as u32,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x20000..=0x2FA1F
    )
}

fn is_cjk_punctuation(character: char) -> bool {
    matches!(
        character,
        '\u{3002}' | '\u{FF0C}' | '\u{FF1F}' | '\u{FF01}' | '\u{FF1A}' | '\u{FF1B}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctc_decode_collapses_repeats_but_preserves_tokens_separated_by_blank() {
        let vocab = vec![
            "\u{2581}\u{4F60}".to_string(),
            "\u{2581}\u{597D}".to_string(),
        ];
        let logits = vec![
            5.0, 0.0, 0.0, // token 0
            5.0, 0.0, 0.0, // repeated token 0
            0.0, 0.0, 5.0, // blank
            5.0, 0.0, 0.0, // token 0 again
            0.0, 5.0, 0.0, // token 1
        ];

        let result = decode_ctc_logits(&logits, 5, 3, 5, &vocab, 2).unwrap();

        assert_eq!(result.text, "\u{4F60}\u{4F60}\u{597D}");
        assert_eq!(result.tokens.len(), 3);
        assert_eq!(result.timestamps, vec![0.0, 0.24, 0.32]);
    }

    #[test]
    fn ctc_decode_ignores_padded_time_steps() {
        let vocab = vec!["a".to_string()];
        let logits = vec![1.0, 0.0, 0.0, 1.0];

        let result = decode_ctc_logits(&logits, 2, 2, 1, &vocab, 1).unwrap();

        assert_eq!(result.text, "a");
    }

    #[test]
    fn text_normalization_keeps_english_spaces_and_removes_chinese_token_spaces() {
        assert_eq!(
            normalize_ctc_text("\u{4F60} \u{597D} OpenAI \u{6A21} \u{578B} \u{3002}"),
            "\u{4F60}\u{597D} OpenAI \u{6A21}\u{578B}\u{3002}"
        );
    }

    #[test]
    fn chunk_join_removes_spaces_around_chinese_punctuation() {
        assert_eq!(
            join_chunk_text(&["\u{4F60}\u{597D}\u{3002}".into(), "\u{4E16}\u{754C}".into()]),
            "\u{4F60}\u{597D}\u{3002}\u{4E16}\u{754C}"
        );
    }
}
