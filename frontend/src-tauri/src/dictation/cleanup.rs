use once_cell::sync::Lazy;
use regex::Regex;
use std::future::Future;
use std::time::Duration;

const LOCAL_CLEANUP_BUDGET: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupFallbackReason {
    TimedOut,
    Failed,
    EmptyResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupResult {
    pub text: String,
    pub fallback_reason: Option<CleanupFallbackReason>,
}

pub async fn cleanup_transcript(raw: String, language: Option<String>) -> CleanupResult {
    let source = raw.clone();
    cleanup_with_budget(raw, LOCAL_CLEANUP_BUDGET, async move {
        Ok::<String, String>(polish_locally(&source, language.as_deref()))
    })
    .await
}

async fn cleanup_with_budget<F, E>(raw: String, budget: Duration, operation: F) -> CleanupResult
where
    F: Future<Output = Result<String, E>>,
{
    match tokio::time::timeout(budget, operation).await {
        Err(_) => raw_fallback(raw, CleanupFallbackReason::TimedOut),
        Ok(Err(_)) => raw_fallback(raw, CleanupFallbackReason::Failed),
        Ok(Ok(cleaned)) if cleaned.trim().is_empty() => {
            raw_fallback(raw, CleanupFallbackReason::EmptyResult)
        }
        Ok(Ok(cleaned)) => CleanupResult {
            text: cleaned,
            fallback_reason: None,
        },
    }
}

fn raw_fallback(raw: String, reason: CleanupFallbackReason) -> CleanupResult {
    CleanupResult {
        text: raw,
        fallback_reason: Some(reason),
    }
}

fn polish_locally(raw: &str, language: Option<&str>) -> String {
    static ENGLISH_FILLERS: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?i)\b(?:um+|uh+|erm+)\b(?:\s*,)?").expect("valid filler regex"));
    static WHITESPACE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"\s+").expect("valid whitespace regex"));
    static SPACE_BEFORE_PUNCTUATION: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"\s+([,.;:!?%\)\]\}])").expect("valid closing punctuation regex"));
    static SPACE_AFTER_OPENING: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"([\(\[\{])\s+").expect("valid opening punctuation regex"));

    let without_fillers = if uses_english_cleanup(language) {
        ENGLISH_FILLERS.replace_all(raw, " ").into_owned()
    } else {
        raw.to_owned()
    };
    let collapsed = WHITESPACE.replace_all(&without_fillers, " ");
    let closed = SPACE_BEFORE_PUNCTUATION.replace_all(&collapsed, "$1");
    SPACE_AFTER_OPENING
        .replace_all(&closed, "$1")
        .trim()
        .to_owned()
}

fn uses_english_cleanup(language: Option<&str>) -> bool {
    let Some(language) = language.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    language.eq_ignore_ascii_case("auto")
        || language.eq_ignore_ascii_case("english")
        || language.eq_ignore_ascii_case("en")
        || language.get(..3).is_some_and(|prefix| {
            prefix.eq_ignore_ascii_case("en-") || prefix.eq_ignore_ascii_case("en_")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn removes_english_fillers_and_repairs_spacing() {
        let result = cleanup_transcript(
            "  Um, this is   uh a test , with spacing.  ".into(),
            Some("en".into()),
        )
        .await;

        assert_eq!(result.text, "this is a test, with spacing.");
        assert_eq!(result.fallback_reason, None);
    }

    #[tokio::test]
    async fn preserves_fillers_for_an_explicit_non_english_language() {
        let result = cleanup_transcript("  um   dia , bonito  ".into(), Some("pt".into())).await;

        assert_eq!(result.text, "um dia, bonito");
        assert_eq!(result.fallback_reason, None);
    }

    #[tokio::test]
    async fn timeout_returns_the_exact_raw_transcript() {
        let raw = " keep   every raw word ".to_string();
        let result = cleanup_with_budget(raw.clone(), Duration::from_millis(5), async {
            tokio::time::sleep(Duration::from_millis(30)).await;
            Ok::<String, String>("late cleanup".into())
        })
        .await;

        assert_eq!(result.text, raw);
        assert_eq!(
            result.fallback_reason,
            Some(CleanupFallbackReason::TimedOut)
        );
    }

    #[tokio::test]
    async fn cleanup_failure_returns_the_exact_raw_transcript() {
        let raw = "raw transcript".to_string();
        let result = cleanup_with_budget(raw.clone(), Duration::from_millis(20), async {
            Err::<String, String>("cleanup unavailable".into())
        })
        .await;

        assert_eq!(result.text, raw);
        assert_eq!(result.fallback_reason, Some(CleanupFallbackReason::Failed));
    }

    #[tokio::test]
    async fn empty_cleanup_result_returns_the_exact_raw_transcript() {
        let raw = "raw transcript".to_string();
        let result = cleanup_with_budget(raw.clone(), Duration::from_millis(20), async {
            Ok::<String, String>("   ".into())
        })
        .await;

        assert_eq!(result.text, raw);
        assert_eq!(
            result.fallback_reason,
            Some(CleanupFallbackReason::EmptyResult)
        );
    }
}
