/// Extracts a snippet of text around the first match of a query.
/// Counts characters, not bytes, so multi-byte text is never cut mid-character.
pub(crate) fn match_context(text: &str, query: &str) -> String {
    const CONTEXT_CHARS: usize = 100;
    let text_lower = text.to_lowercase();
    let query_lower = query.to_lowercase();

    let Some(match_byte) = text_lower.find(&query_lower) else {
        return text.chars().take(200).collect(); // Fallback to the start of the text
    };

    let total_chars = text.chars().count();
    let match_char = text_lower[..match_byte].chars().count().min(total_chars);
    let start = match_char.saturating_sub(CONTEXT_CHARS);
    let end = (match_char + query.chars().count() + CONTEXT_CHARS).min(total_chars);

    let mut context = String::new();
    if start > 0 {
        context.push_str("...");
    }
    context.extend(text.chars().skip(start).take(end - start));
    if end < total_chars {
        context.push_str("...");
    }
    context
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_context_survives_multi_byte_text() {
        // 67 three-byte characters put the 100-byte look-back in the middle of a character.
        let text = format!("{}budget review 🎯", "日".repeat(67));
        let context = match_context(&text, "budget");
        assert!(context.contains("budget review 🎯"));
    }

    #[test]
    fn match_context_trims_long_text_with_ellipses() {
        let text = format!("{}needle{}", "a".repeat(300), "b".repeat(300));
        let context = match_context(&text, "NEEDLE");
        assert!(context.starts_with("..."));
        assert!(context.ends_with("..."));
        assert!(context.contains("needle"));
        assert_eq!(context.chars().count(), 3 + 100 + 6 + 100 + 3);
    }
}
