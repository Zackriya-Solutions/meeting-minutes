// audio/transcription/gemini_live/dedup.rs
//
// After a reconnect the new session is primed with the tail of the previous segment's
// audio, so the first result on the new connection can repeat words that were already
// emitted. `trim_overlap` removes the longest run of words that ends `previous` and
// starts `next`, comparing case- and punctuation-insensitively.

const MIN_OVERLAP_WORDS: usize = 2;
const MAX_OVERLAP_WORDS: usize = 20;

fn normalize(word: &str) -> String {
    // Devanagari vowel signs are combining marks, not alphanumeric, so keep the whole
    // block except the danda punctuation.
    let is_devanagari = |c: char| ('\u{0900}'..='\u{097F}').contains(&c) && c != '।' && c != '॥';
    word.chars()
        .filter(|&c| c.is_alphanumeric() || is_devanagari(c))
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn trim_overlap(previous: &str, next: &str) -> String {
    let prev: Vec<String> = previous.split_whitespace().map(normalize).collect();
    let next_words: Vec<&str> = next.split_whitespace().collect();
    let next_norm: Vec<String> = next_words.iter().map(|w| normalize(w)).collect();

    let max_k = MAX_OVERLAP_WORDS.min(prev.len()).min(next_norm.len());
    for k in (MIN_OVERLAP_WORDS..=max_k).rev() {
        let tail = &prev[prev.len() - k..];
        let head = &next_norm[..k];
        if tail.iter().zip(head).all(|(a, b)| !a.is_empty() && a == b) {
            return next_words[k..].join(" ");
        }
    }
    next.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_repeated_tail() {
        assert_eq!(
            trim_overlap(
                "Rahul, kya aap Friday tak ye fix kar sakte ho?",
                "fix kar sakte ho? Aur haan, client ne bola hai"
            ),
            "Aur haan, client ne bola hai"
        );
    }

    #[test]
    fn ignores_case_and_punctuation() {
        assert_eq!(trim_overlap("sabko ready rehna hai.", "Ready rehna hai, koi blockers?"), "koi blockers?");
    }

    #[test]
    fn keeps_text_without_overlap() {
        assert_eq!(trim_overlap("Okay team.", "Chaliye start karte hain."), "Chaliye start karte hain.");
    }

    #[test]
    fn single_word_match_is_not_enough() {
        // "hai" ending one sentence and starting another is common; don't eat it.
        assert_eq!(trim_overlap("demo Tuesday ko hai", "hai na sab ready?"), "hai na sab ready?");
    }

    #[test]
    fn full_duplicate_becomes_empty() {
        assert_eq!(trim_overlap("koi blockers hain?", "Koi blockers hain?"), "");
    }

    #[test]
    fn works_with_devanagari() {
        assert_eq!(trim_overlap("सबको रेडी रहना है।", "रेडी रहना है। कोई ब्लॉकर्स हैं?"), "कोई ब्लॉकर्स हैं?");
    }
}
