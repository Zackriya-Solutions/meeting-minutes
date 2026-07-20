# Wave 28: LLM Failure UX (PR-45 a/b)

> **Base branch:** devtest
> **Parent waves:** PR-42-iv (typed `LLMError`), PR-23 (hotword stats panel pattern), PR-44c (i18n layout).
> **Goal:** Give users actionable feedback when LLM postprocess fails — a settings-side diagnostics panel plus per-segment inline retry — without re-architecting the realtime pipeline.

## Background

PR-42-iv typed `LLMError` and forwarded a `{code, message}` payload to the frontend (`useTranscriptPostprocessEvents`). Today the frontend just renders a tooltip on the failed segment and logs a warning. Users have:

- No way to verify their provider config is correct before recording a long meeting.
- No aggregate view of which errors keep recurring.
- No affordance to retry a single failed segment without disabling postprocess globally.

This wave keeps the realtime pipeline untouched and adds two thin UX layers on top.

## Non-Goals

- No changes to `LLMError` variants or the typed contract (forward-compatible).
- No persistence of the diagnostics list to SQLite; in-memory only.
- No automatic provider failover / suggestion. We surface a hint string per code; the user chooses.
- No change to the cancellation flow (`Cancelled` errors never retry).
- No model downgrade / fallback; out of scope.

## Decisions

| Question | Choice | Why |
|---|---|---|
| Test scope | Settings page only | User asked for **C** (settings + inline retry), no global toast |
| Diagnostics storage | In-memory `Mutex<VecDeque>` (cap 200) | Cheap; matches HotwordHitStatsPanel semantics; reset on restart |
| Inline retry trigger | Hover-only ghost button | Avoids visual noise on every line |
| Retry path | Existing `spawn_segment_postprocess` | Reuses the typed-error path, no new engine plumbing |
| i18n keys | Add to existing `settings.json` + `transcript.json` | No new locale file needed |

## Architecture

```
Settings page
  └── "Test connection" button
        └── test_llm_connection() -> {ok, latency_ms, code?, message?}
              ├── success: emit llm-diagnostics-updated (last_test)
              └── failure: append to diagnostics; emit event
                                              │
LLMDiagnosticsPanel (subscribes to llm-diagnostics-updated)
  └── per-code counts + per-code i18n suggestion
  └── "Clear" button -> clear_llm_diagnostics()
                                              │
Transcript row (only when postprocess_failed)
  └── hover ghost "Retry" icon
        └── retry_segment_postprocess(meeting_id, segment_id, text)
              └── spawn_segment_postprocess (existing)
                    ├── on success: transcript-postprocessed
                    └── on failure: transcript-postprocess-failed (idempotent)
```

## Scope (2 PRs)

### PR-45a — Backend diagnostics + test command

| File | Change |
|---|---|
| `frontend/src-tauri/src/llm_diagnostics.rs` (new) | `LLMDiagnosticsState` (Mutex<VecDeque>), `record_failure(code)`, `buckets()`, `clear()`, `record_last_test(...)` |
| `frontend/src-tauri/src/llm_postprocess.rs` | Push failure events through `LLMDiagnosticsState` after every `map_llm_error`; expose `test_llm_connection` and `retry_segment_postprocess` |
| `frontend/src-tauri/src/lib.rs` | Register `test_llm_connection`, `get_llm_diagnostics`, `clear_llm_diagnostics`, `retry_segment_postprocess`; emit `llm-diagnostics-updated` |
| `frontend/src-tauri/src/diarization/mod.rs` | Re-export pattern only if needed (not required) |
| Tests | `tests_diagnostics.rs` covers bucket aggregation + last-test bookkeeping |

### PR-45b — Frontend diagnostics panel + inline retry

| File | Change |
|---|---|
| `frontend/src/hooks/useLLMDiagnostics.ts` (new) | Subscribes to `llm-diagnostics-updated`; exposes `buckets`, `lastTest`, `refresh()`, `clear()` |
| `frontend/src/components/LLMDiagnosticsPanel.tsx` (new) | Lists buckets with i18n suggestions + last-test result; "Clear" button |
| `frontend/src/components/TranscriptSettings.tsx` | Insert "Test connection" + `<LLMDiagnosticsPanel />` after the model-provider block |
| `frontend/src/components/VirtualizedTranscriptView.tsx` | Add hover-only `Retry` icon when `postprocess_failed === true` |
| `frontend/src/types/index.ts` | Add `RetrySegmentPostprocessResult` |
| `frontend/locales/{6 locale}/settings.json` | `llm.test_connection`, `llm.test_connection.ok`, `llm.test_connection.failed`, `llm.diagnostics.*` (10 keys) |
| `frontend/locales/{6 locale}/transcript.json` | `transcript.retry_postprocess`, `transcript.retry_postprocess_tooltip`, `transcript.retry_postprocess_disabled_tooltip`, `transcript.retry_postprocess_failed_toast` |

## Acceptance

### PR-45a

- [ ] `test_llm_connection` returns `{ok: true, latency_ms}` for a healthy Ollama instance; `{ok: false, code: 'auth_failed', message}` for a 401.
- [ ] Every `map_llm_error` call records one entry into `LLMDiagnosticsState`.
- [ ] `buckets()` returns up to 6 code buckets, sorted by descending count.
- [ ] `clear_llm_diagnostics` empties the in-memory list and emits the event.
- [ ] `retry_segment_postprocess` reuses `spawn_segment_postprocess` and accepts `(meeting_id, segment_id, text)`.
- [ ] `cargo test --lib llm_diagnostics` passes.

### PR-45b

- [ ] "Test connection" button shows a spinner + result line within 2 s of click; result string uses i18n key `llm.test_connection.{ok,failed}`.
- [ ] `LLMDiagnosticsPanel` renders below the model block; lists non-zero buckets in i18n order; "Clear" button only renders when at least one bucket has count > 0.
- [ ] `VirtualizedTranscriptView` shows the `Retry` icon only on failed segments (postprocess_failed === true); click fires `retry_segment_postprocess`; success or failure updates the row state via existing events.
- [ ] 6 locales contain every new key; `pnpm check:i18n` + `pnpm test:i18n` pass.
- [ ] `pnpm build` succeeds.

## Risks

| Risk | Mitigation |
|---|---|
| `test_llm_connection` triggers a real model call (cost / latency) | Use smallest prompt + max_tokens=1; cap to once per 10 s from frontend (button disabled timer) |
| Diagnostics list grows unbounded | Cap at 200 entries; VecDeque::pop_front on overflow |
| Retry click storms one provider | Same backend rate limit (token-bucket reused from PR-42-iv) |
| Mixed languages in provider messages | Use `code` (stable) + i18n hint; never surface raw body in UI |

## References

- PR-42-iv spec: `docs/superpowers/specs/2026-07-19-llm-error-typed-public.md`
- PR-23 (hotword stats): `frontend/src/components/HotwordHitStatsPanel.tsx`
- LLMError: `frontend/src-tauri/src/summary/llm_client.rs`
