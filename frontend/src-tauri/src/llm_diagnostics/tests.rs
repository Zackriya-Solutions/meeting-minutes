use super::*;

#[test]
fn record_failure_increments_bucket() {
    let state = LLMDiagnosticsState::default();
    state.record_failure("auth_failed", "401");
    state.record_failure("auth_failed", "401 again");
    state.record_failure("rate_limited", "429");
    let buckets = state.buckets();
    assert_eq!(buckets.len(), 2);
    assert_eq!(buckets[0].code, "auth_failed");
    assert_eq!(buckets[0].count, 2);
    assert_eq!(buckets[1].code, "rate_limited");
    assert_eq!(buckets[1].count, 1);
}

#[test]
fn cap_evicts_oldest_entries() {
    let state = LLMDiagnosticsState::default();
    for _ in 0..(DIAGNOSTICS_CAP + 5) {
        state.record_failure("internal", "x");
    }
    let buckets = state.buckets();
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].count, DIAGNOSTICS_CAP);
}

#[test]
fn last_test_round_trip() {
    let state = LLMDiagnosticsState::default();
    assert!(state.last_test().is_none());
    state.set_last_test(LastTestResult::ok(42));
    let t = state.last_test().expect("set");
    assert!(t.ok);
    assert_eq!(t.latency_ms, 42);
    state.set_last_test(LastTestResult::failed(100, "auth_failed", "401"));
    let t = state.last_test().expect("set");
    assert!(!t.ok);
    assert_eq!(t.code.as_deref(), Some("auth_failed"));
}

#[test]
fn clear_keeps_last_test() {
    let state = LLMDiagnosticsState::default();
    state.record_failure("network", "x");
    state.set_last_test(LastTestResult::ok(7));
    state.clear();
    assert!(state.buckets().is_empty());
    assert!(state.last_test().is_some());
}

#[test]
fn clear_all_resets_everything() {
    let state = LLMDiagnosticsState::default();
    state.record_failure("network", "x");
    state.set_last_test(LastTestResult::ok(7));
    state.clear_all();
    assert!(state.buckets().is_empty());
    assert!(state.last_test().is_none());
}
