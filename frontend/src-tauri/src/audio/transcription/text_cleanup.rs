// audio/transcription/text_cleanup.rs
//
// Shared post-processing for ASR output, applied to EVERY transcription engine.
//
// These routines previously lived as private associated functions on
// `WhisperEngine`, so they only ran for local Whisper. Parakeet and the
// trait-based providers stored raw model output, which meant Whisper's
// well-known subtitle-corpus hallucinations were filtered on one code path and
// persisted verbatim on the others.
//
// The denylist was also English-only, so the filter silently applied to English
// meetings and not to any other language. Whisper emits the same class of
// artifact in whatever language it decided it was hearing, so the patterns are
// grouped by language below and every group is always checked.

use std::collections::{HashMap, HashSet};

/// Minimum words before repetition analysis is meaningful.
const MIN_WORDS_FOR_REPETITION_CHECK: usize = 3;
/// Minimum words before phrase-level repetition analysis is meaningful.
const MIN_WORDS_FOR_PHRASE_CHECK: usize = 4;
/// Longest phrase length considered when collapsing repeated phrases.
const MAX_PHRASE_LEN: usize = 5;
/// Above this share of repeated words the output is treated as degenerate.
const MAX_REPETITION_RATIO: f32 = 0.7;
/// A short string built from this few distinct characters is not speech.
const MAX_UNIQUE_CHARS_FOR_JUNK: usize = 3;
/// Only apply the distinct-character heuristic above this length.
const MIN_LEN_FOR_UNIQUE_CHAR_CHECK: usize = 10;

/// Subtitle/voice-over boilerplate that Whisper reproduces from its training
/// data when handed audio with little or no speech in it. Matched
/// case-insensitively as a substring.
///
/// English entries are the original list. The remaining entries were collected
/// from real recordings; the Arabic ones in particular are what a
/// Whisper-transcribed Arabic meeting fills near-silent segments with.
const BOILERPLATE_PATTERNS: &[&str] = &[
    // --- English ---
    "thank you for watching",
    "thanks for watching",
    "like and subscribe",
    "please subscribe",
    "subscribe to the channel",
    "music playing",
    "applause",
    "laughter",
    "um um um",
    "uh uh uh",
    "ah ah ah",
    // --- Arabic ---
    // "subscribe to the channel" / "and subscribe to the channel"
    "اشتركوا في القناة",
    "اشترك في القناة",
    "نشترك في القناة",
    "اشتركوا بالقناة",
    // "translation by ..." / "subtitles by ..." subtitler credits
    "ترجمة نانسي قنقر",
    "ترجمة نانسي",
    "ترجمة وتعديل",
    "ترجمة الفيديو",
    "تمت الترجمة",
    // ad / outro boilerplate
    "استمتعوا بالإنترنت",
    "شكرا على المشاهدة",
    "شكراً على المشاهدة",
    "لا تنسوا الاشتراك",
];

/// Standalone filler that is only ever a hallucination when it constitutes the
/// entire segment. Unlike [`BOILERPLATE_PATTERNS`] these are matched against the
/// whole trimmed string, because the same words are legitimate mid-sentence.
///
/// Whisper's Arabic "شكرا" attractor is the direct analogue of its English
/// "Thank you." attractor on near-silent input.
const STANDALONE_FILLER: &[&str] = &[
    // English
    "thank you",
    "thank you.",
    "thanks",
    "you",
    "bye",
    "okay",
    // Arabic — "thank you" in its common orthographic variants
    "شكرا",
    "شكراً",
    "شكرا.",
    "شكراً.",
    "شكرا لك",
    "شكراً لك",
    "شكرا لكم",
    "شكراً لكم",
    "شكرا جزيلا",
    "شكراً جزيلاً",
];

/// Strip trailing punctuation and Arabic tatweel so orthographic variants of the
/// same filler word collapse to one form for comparison.
fn normalize_for_filler_match(text: &str) -> String {
    text.trim()
        .trim_end_matches(|c: char| matches!(c, '.' | '!' | '?' | '،' | '؟' | '۔' | ',' | ' '))
        .replace('\u{0640}', "") // ARABIC TATWEEL
        .to_lowercase()
}

/// True when `text` is a whole-segment hallucination rather than speech.
pub fn is_meaningless_output(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }

    let lower = trimmed.to_lowercase();
    if BOILERPLATE_PATTERNS
        .iter()
        .any(|pattern| lower.contains(pattern))
    {
        return true;
    }

    let normalized = normalize_for_filler_match(trimmed);
    if STANDALONE_FILLER
        .iter()
        .any(|filler| normalized == normalize_for_filler_match(filler))
    {
        return true;
    }

    // A long string drawn from a handful of distinct characters is not speech.
    let unique_chars: HashSet<char> = trimmed.chars().collect();
    if unique_chars.len() <= MAX_UNIQUE_CHARS_FOR_JUNK
        && trimmed.len() > MIN_LEN_FOR_UNIQUE_CHAR_CHECK
    {
        return true;
    }

    false
}

/// Collapse runs of an identical repeated word down to a single instance.
fn remove_word_repetitions<'a>(words: &'a [&'a str]) -> Vec<&'a str> {
    let mut cleaned_words = Vec::new();
    let mut i = 0;

    while i < words.len() {
        let current_word = words[i];
        let mut repeat_count = 1;

        while i + repeat_count < words.len() && words[i + repeat_count] == current_word {
            repeat_count += 1;
        }

        cleaned_words.push(current_word);
        i += repeat_count;
    }

    cleaned_words
}

/// Collapse an immediately repeated phrase (2..=5 words) down to one instance.
fn remove_phrase_repetitions<'a>(words: &'a [&'a str]) -> Vec<&'a str> {
    if words.len() < MIN_WORDS_FOR_PHRASE_CHECK {
        return words.to_vec();
    }

    let mut final_words = Vec::new();
    let mut i = 0;

    while i < words.len() {
        let mut phrase_found = false;

        for phrase_len in 2..=std::cmp::min(MAX_PHRASE_LEN, (words.len() - i) / 2) {
            if i + phrase_len * 2 <= words.len() {
                let phrase1 = &words[i..i + phrase_len];
                let phrase2 = &words[i + phrase_len..i + phrase_len * 2];

                if phrase1 == phrase2 {
                    final_words.extend_from_slice(phrase1);
                    i += phrase_len * 2;
                    phrase_found = true;
                    break;
                }
            }
        }

        if !phrase_found {
            final_words.push(words[i]);
            i += 1;
        }
    }

    final_words
}

/// Share of words that are repeats of an earlier word, in 0.0..=1.0.
fn calculate_repetition_ratio(text: &str) -> f32 {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < MIN_WORDS_FOR_PHRASE_CHECK {
        return 0.0;
    }

    let mut word_counts = HashMap::new();
    for word in &words {
        *word_counts.entry(word.to_lowercase()).or_insert(0usize) += 1;
    }

    let total_words = words.len() as f32;
    let repeated_words: usize = word_counts
        .values()
        .map(|&count| count.saturating_sub(1))
        .sum();

    repeated_words as f32 / total_words
}

/// Clean one segment of ASR output. Returns an empty string when the segment
/// should be discarded entirely.
///
/// Safe to call on output from any engine and in any language.
pub fn clean_transcript_text(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if is_meaningless_output(trimmed) {
        return String::new();
    }

    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.len() < MIN_WORDS_FOR_REPETITION_CHECK {
        return trimmed.to_string();
    }

    let cleaned_words = remove_word_repetitions(&words);
    let cleaned_words = remove_phrase_repetitions(&cleaned_words);

    let final_text = cleaned_words.join(" ");
    if calculate_repetition_ratio(&final_text) > MAX_REPETITION_RATIO {
        return String::new();
    }

    // Re-check after collapsing: a repeated hallucination such as
    // "شكرا شكرا شكرا" only becomes recognisable as standalone filler once the
    // repetition has been removed.
    if is_meaningless_output(&final_text) {
        return String::new();
    }

    final_text
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- the artifacts this module exists to remove -------------------------

    #[test]
    fn drops_arabic_subscribe_boilerplate() {
        assert_eq!(clean_transcript_text("اشتركوا في القناة"), "");
        assert_eq!(clean_transcript_text("ونشترك في القناة"), "");
    }

    #[test]
    fn drops_arabic_subtitler_credit() {
        assert_eq!(clean_transcript_text("ترجمة نانسي قنقر"), "");
    }

    #[test]
    fn drops_standalone_arabic_thank_you_in_all_orthographies() {
        for variant in ["شكرا", "شكراً", "شكرا لك", "شكراً جزيلاً", "شكرا."] {
            assert_eq!(
                clean_transcript_text(variant),
                "",
                "expected {variant:?} to be filtered"
            );
        }
    }

    #[test]
    fn drops_english_boilerplate_as_before() {
        assert_eq!(clean_transcript_text("Thanks for watching!"), "");
        assert_eq!(clean_transcript_text("Please subscribe"), "");
    }

    // --- and what it must NOT remove ---------------------------------------

    #[test]
    fn keeps_arabic_thank_you_inside_a_real_sentence() {
        let sentence = "شكرا لك على هذا العرض الممتاز سنراجعه غدا";
        assert_eq!(clean_transcript_text(sentence), sentence);
    }

    #[test]
    fn keeps_ordinary_arabic_speech() {
        let sentence = "الرسائل هذه لابد أن يكون فيها أربع أشياء";
        assert_eq!(clean_transcript_text(sentence), sentence);
    }

    #[test]
    fn keeps_code_switched_arabic_english() {
        let sentence = "لازم تكون في كل رسالة اللي هي four components";
        assert_eq!(clean_transcript_text(sentence), sentence);
    }

    #[test]
    fn keeps_short_genuine_replies() {
        // Two words, below the repetition threshold — must pass through.
        assert_eq!(clean_transcript_text("نعم صحيح"), "نعم صحيح");
    }

    // --- repetition behaviour ---------------------------------------------

    #[test]
    fn collapses_repeated_words() {
        assert_eq!(
            clean_transcript_text("the cat cat cat sat down"),
            "the cat sat down"
        );
    }

    #[test]
    fn collapses_repeated_phrases() {
        assert_eq!(
            clean_transcript_text("we need to adopt we need to adopt this change"),
            "we need to adopt this change"
        );
    }

    #[test]
    fn discards_fully_degenerate_repetition() {
        assert_eq!(clean_transcript_text("شكرا شكرا شكرا شكرا شكرا"), "");
    }

    #[test]
    fn empty_and_whitespace_are_empty() {
        assert_eq!(clean_transcript_text(""), "");
        assert_eq!(clean_transcript_text("   \n "), "");
    }

    #[test]
    fn single_character_runs_are_junk() {
        assert!(is_meaningless_output("aaaaaaaaaaaaaa"));
        assert!(is_meaningless_output("............."));
    }

    #[test]
    fn tatweel_variants_normalize_to_the_same_filler() {
        // ARABIC TATWEEL stretches a word without changing it.
        assert_eq!(clean_transcript_text("شكـــرا"), "");
    }
}
