//! Provider routing (PLAN.md Phase 4): single-meeting / lookup questions favor a fast
//! provider (GigaChat), cross-meeting synthesis favors a stronger one (DeepSeek).
//! The decision is abstract ([`RouteTarget`]); the caller maps it to whichever provider
//! is actually configured (e.g. DeepSeek via the custom-OpenAI endpoint). Routing
//! decisions are logged for later tuning, as the plan requests.

/// Scope of a retrieval/answer request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    SingleMeeting,
    Collection,
    Archive,
}

/// Abstract routing outcome. `Fast` ≈ GigaChat (lookup), `Synthesis` ≈ DeepSeek
/// (cross-meeting reasoning / structured extraction).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteTarget {
    Fast,
    Synthesis,
}

/// Heuristic query-length threshold (chars) above which even single-scope questions are
/// treated as synthesis.
const LONG_QUERY_CHARS: usize = 160;

/// Choose a routing target from purpose + scope + query length. Deterministic and pure.
pub fn route(purpose: super::Purpose, scope: Scope, query_chars: usize) -> RouteTarget {
    use super::Purpose;
    let target = match purpose {
        // Structured extraction always uses the stronger synthesis model.
        Purpose::Extract => RouteTarget::Synthesis,
        // Summaries are single-meeting; keep them on the fast path.
        Purpose::Summary => RouteTarget::Fast,
        // Chat routes by scope + length.
        Purpose::Chat => match scope {
            Scope::SingleMeeting if query_chars <= LONG_QUERY_CHARS => RouteTarget::Fast,
            _ => RouteTarget::Synthesis,
        },
    };
    log::debug!(
        "llm route: purpose={} scope={:?} query_chars={} -> {:?}",
        purpose.as_str(),
        scope,
        query_chars,
        target
    );
    target
}

#[cfg(test)]
mod tests {
    use super::super::Purpose;
    use super::*;

    #[test]
    fn extraction_always_synthesis() {
        assert_eq!(
            route(Purpose::Extract, Scope::SingleMeeting, 10),
            RouteTarget::Synthesis
        );
    }

    #[test]
    fn single_meeting_short_chat_is_fast() {
        assert_eq!(
            route(Purpose::Chat, Scope::SingleMeeting, 20),
            RouteTarget::Fast
        );
    }

    #[test]
    fn archive_or_long_chat_is_synthesis() {
        assert_eq!(
            route(Purpose::Chat, Scope::Archive, 20),
            RouteTarget::Synthesis
        );
        assert_eq!(
            route(Purpose::Chat, Scope::SingleMeeting, 500),
            RouteTarget::Synthesis,
            "long query escalates even single-meeting scope"
        );
    }
}
