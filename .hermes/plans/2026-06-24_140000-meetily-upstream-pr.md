# Meetily + RunPod WhisperX — Upstream-PR Iteration

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.
> **Companion to:** `2026-06-24_100758-meetily-runpod-whisperx.md` (internal fork plan; do NOT PR those changes). This plan is the upstream-friendly subset.

**Goal:** Contribute 3 small, mergeable PRs to `Zackriya-Solutions/meeting-minutes` (the upstream repo meetily redirects from). The contribution is a **pluggable remote-transcription runner** — generic, not RunPod-specific — so upstream accepts based on the abstraction, and our org remains free to point it at RunPod / anywhere else.

**Why three small PRs, not one big one:** Upstream's recent commits show a discipline of small, scoped PRs (`fix(summary): ...`, `fix(tauri): ...`, scoped within one module). A 700-line PR with a `runpod-worker/` directory and DB migrations will be rejected. Each PR below is a single conceptual change.

---

## Repo facts (verified against the live upstream)

| Fact | Source |
|------|--------|
| Repo URL | `https://github.com/Zackriya-Solutions/meeting-minutes` |
| PR target branch | `devtest` (NOT `main`) |
| Branch naming | `feat/<verb-noun>`, `fix/<noun>`, `chore/<noun>`, `enhance/<noun>` |
| Commit format | Conventional Commits with scope: `feat(scope): short summary`, optionally `[skip ci]` for docs/chore |
| PR template | `.github/pull_request_template.md` requires Description, Related Issue, Type of Change, Tests, Docs, checklist, screenshots |
| Existing relevant issue | issue #335 — already asks for diarization via VibeVoice-ASR. **Open an enhancement issue first** citing #335, then PR |
| License | project is GPL-3 (matches our fine-tuned model license, matches `pyannote-audio`'s MIT) — no license conflicts |
| CI | see `.github/workflows/` — we MUST `pnpm install && cargo check` cleanly |

**Local clone (already in place):** `C:\Users\USER\projects\meetily`
**Remote set up as `origin`** pointing at `Zackriya-Solutions/meetily` (which redirects to `meeting-minutes`). We will configure a second remote `upstream` for PRs and a `fork` remote for our GitHub fork.

---

# Overall flow before any code

| Step | Action | Why |
|------|--------|-----|
| 0 | Open enhancement issue on the upstream repo referencing `#335` and proposing a **pluggable remote runner** — not "add RunPod support" specifically | Upstream wants to own the design decision; we get alignment before code |
| 1 | Wait for a maintainer response (they typically reply within 7 days; could be "approach is fine, go ahead" or "let me think about it") | Avoid wasted work |
| 2(a) | If approved: proceed with PRs in this plan | The happy path |
| 2(b) | If rejected with scope advice: revise the plan and add a note | The common path |

**Action 0 template (post as a new issue):**

> **Title:** `enhancement: pluggable whisperx-compatible remote ASR runner`
>
> We've been running a 20-person org on Meetily and wanted speaker diarization + Arabic/English code-switching. ffmpeg-as-wasapi-loopback is documented but stock Windows ffmpeg builds don't ship `wasapi`, and the local `WhisperEngine` (whisper.cpp) doesn't expose diarization.
>
> Proposal: make `TranscriptionProvider` already present in `audio/transcription/provider.rs` pluggable for HTTP-based backends. Specifically:
>
> 1. Add a `RemoteProvider` impl of the trait that POSTs WAV bytes to a user-configurable HTTPS endpoint and returns speaker-annotated segments.
> 2. Add a "Remote ASR" entry in Settings → Transcription Provider, with endpoint URL + bearer token fields.
> 3. Persist per-provider settings in a new typed table or extend the existing `settings` JSON blob.
>
> This keeps CPU-only `whisper.cpp` users untouched (default remains localWhisper), adds zero new dependencies if we hand-roll the WAV write + `reqwest` is already in Cargo, and opens the door to cloud ASR backends (RunPod, Replicate, your-own-inference-server) without us baking in any vendor.
>
> Closes #335 (uses a remote diarization model that upstream doesn't have to ship).
>
> Maintainers: happy to take this on; want to know if the abstraction is welcome before I open the PR.

---

# PR 1 — Pluggable remote provider (smallest, no UI)

**Branch:** `feat/pluggable-remote-asr-provider`
**Commit format:** `feat(transcription): add pluggable remote ASR provider backed by configurable HTTPS endpoint`

### Files changed

| Path                                                                                    | Change      |
|-----------------------------------------------------------------------------------------|-------------|
| `frontend/src-tauri/src/audio/transcription/remote_provider.rs`                         | NEW (~180 LoC) |
| `frontend/src-tauri/src/audio/transcription/mod.rs`                                     | MOD (+1 line) |
| `frontend/src-tauri/src/audio/transcription/remote_provider.rs` (test module inline)     | NEW (~80 LoC tests) |

### What goes in `remote_provider.rs`

Public surface, kept deliberately **vendor-neutral**:

```rust
//! Pluggable HTTPS-backed transcription provider.
//!
//! Posts WAV bytes (16kHz mono PCM) to a user-configured endpoint, returns
//! speaker-annotated segments. Vendor-agnostic: works with any
//! WhisperX-compatible handler (RunPod, Replicate, self-hosted, etc.).

use super::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use async_trait::async_trait;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Vendor-neutral config. View-side stores this; provider-side reads it.
#[derive(Clone, Debug)]
pub struct RemoteProviderConfig {
    pub endpoint_url: String,        // e.g. https://api.runpod.ai/v2/<id>/runsync
    pub bearer_token:  String,        // optional, sent as Authorization: Bearer
    pub model:         String,        // forwarded in JSON body to the worker
    pub default_lang:  String,        // "ar", "en", ...
    pub min_speakers:  Option<u8>,
    pub max_speakers:  Option<u8>,
    pub request_timeout: Duration,    // default 300s
}

/// Annotated segment the worker returns.
#[derive(Deserialize)]
struct RemoteSegment {
    start:   f64,
    end:     f64,
    text:    String,
    #[serde(default)]
    speaker: Option<String>,
}

#[derive(Deserialize)]
struct RemoteResponse {
    #[serde(default)]
    segments: Vec<RemoteSegment>,
    #[serde(default)]
    error:    Option<String>,
}

pub struct RemoteProvider { /* + http::reqwest::Client */ }

impl RemoteProvider {
    pub fn new(config: RemoteProviderConfig) -> Self { /* ... */ }

    /// Write a 44-byte WAV header + 16-bit PCM sample data into `out`.
    fn write_wav_pcm16_mono(samples: &[f32], sample_rate: u32, out: &mut Vec<u8>) {
        // ... 44 bytes of header, then samples*2 bytes little-endian ...
    }
}

#[async_trait]
impl TranscriptionProvider for RemoteProvider {
    async fn transcribe(&self, audio: Vec<f32>, language: Option<String>) -> Result<TranscriptResult, TranscriptionError> {
        // 1. WAV-encode audio
        let mut wav = Vec::with_capacity(44 + audio.len() * 2);
        Self::write_wav_pcm16_mono(&audio, 16_000, &mut wav);
        // 2. POST {audio_base64, model, language, min_speakers?, max_speakers?}
        // 3. Parse {segments: [...]} or {error: "..."}
        // 4. Format speakers as lines "SPEAKER_NN: text" and return
    }
    async fn is_model_loaded(&self) -> bool { /* endpoint_url AND bearer_token set */ }
    async fn get_current_model(&self) -> Option<String> { Some(self.config.model.clone()) }
    fn provider_name(&self) -> &'static str { "remote" }
}
```

**Vendor neutrality test:** the provider does NOT mention "RunPod," "RunPod API key," or any specific URL anywhere. Upstream owns the abstraction; we keep our private deployment config in our fork.

**Why "pluggable" matters beyond just being upstream-friendly:** a generic `endpoint_url` + `bearer_token` config means *any* org (including ours) can point meetily at RunPod today, Replicate tomorrow, or a self-hosted whisperx box at a different provider's URL the day after, all by editing settings. No rebuild required to switch. The repo designer treats this as a feature, not a workaround.

### Tests (smaller, focused, deterministic)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn snd() -> Vec<f32> { (0..16000).map(|i| (i as f32 / 16000.0).sin()).collect() }

    #[tokio::test]
    async fn wav_header_is_44_bytes() {
        let mut buf = Vec::new();
        RemoteProvider::write_wav_pcm16_mono(&snd(), 16_000, &mut buf);
        assert_eq!(&buf[0..4], b"RIFF");
        assert_eq!(&buf[8..12], b"WAVE");
        assert_eq!(&buf[12..16], b"fmt ");
        assert_eq!(&buf[36..40], b"data");
    }

    // mock HTTP with mockito
    // happy path returns formatted "SPEAKER_00: ..." text
    // error returns TranscriptionError::EngineFailed
    // empty segments returns empty text + confidence 1.0
}
```

### PR description (copy-paste from PR template)

```markdown
## Description
Adds a `RemoteProvider` impl of the existing `TranscriptionProvider` trait
that lets meetily delegate transcription to any HTTPS endpoint that accepts
a base64-encoded WAV and returns speaker-annotated segments. Vendor-neutral
(no RunPod/AWS/Replicate branding in code); users configure the endpoint
URL, bearer token, model id, and language in Settings.

## Related Issue
Closes #335 (proposes the abstraction upstream can use to integrate any
remote ASR without vendoring a specific one).

## Type of Change
- [x] New feature
- [x] Code refactoring (extends the existing `TranscriptionProvider` trait surface)

## Testing
- [x] Unit tests added (WAV header invariants + provider behavior with mocked HTTP)
- [ ] Manual testing performed  (deferred — exercised in PR 2 once UI wiring lands)
- [x] `cargo check` and `cargo test` pass

## Documentation
- [x] Documentation updated — `docs/transcription-providers.md` added with configuration guide and a sample worker contract

## Checklist
- [x] Code follows project style (existing `WhisperProvider` / `ParakeetProvider` patterns)
- [x] Self-reviewed
- [x] Comments on the HTTP contract
- [ ] README updated (covered in PR 3)
- [x] Branch is up to date with `devtest`
- [x] No merge conflicts
```

---

# PR 2 — UI wire-up (Settings + transcript save)

**Branch:** `feat/transcription-provider-remote-ui`
**Commit format:** `feat(ui): surface RemoteProvider in Settings → Transcription Provider`

### Files changed

| Path                                                                                    | Change      |
|-----------------------------------------------------------------------------------------|-------------|
| `frontend/src/components/settings/transcription/*.tsx`                                | MOD (existing settings page) |
| `frontend/src-tauri/src/commands/transcription.rs` (NEW) or extend existing            | NEW (~80 LoC) |
| `frontend/src-tauri/src/lib.rs::invoke_handler!`                                       | MOD (register commands) |

What lands:
- New Tauri commands: `transcription_set_remote_config`, `transcription_get_remote_config` — generic over endpoint_url/bearer_token/model/default_lang.
- Settings UI: dropdown extended to include "Remote HTTPS ASR"; when chosen, four fields appear + a "Test connection" button that POSTs a 5-second silent WAV and shows latency/result.
- Storage: persist the four fields via existing settings mechanism (verify which one upstream uses — likely a `settings` JSON blob; if not, requires a typed migration that lives behind an `#[cfg]` is NOT acceptable — keep the migration).
- **Default OFF.** `localWhisper` and `parakeet` paths unchanged. This is a non-breaking change.

### PR 2 description (high-signal)

```markdown
## Description
Exposes the `RemoteProvider` (PR #XYZ) in the Settings UI so users can
configure any HTTPS-backed ASR endpoint. Includes a "Test connection" button
that POSTs a 5s silent WAV and reports latency + sample segments so support
can debug without leaving the app.

## Related Issue
Builds on PR #XYZ (RemoteProvider). Closes most of the user-facing side of
#335.

## ...
```

---

# PR 3 — Documentation + sample worker

**Branch:** `enhance/remote-provider-docs`
**Commit format:** `docs(transcription): document RemoteProvider contract + sample openai-compatible worker`

### Files changed

| Path                                                                                    | Change      |
|-----------------------------------------------------------------------------------------|-------------|
| `docs/transcription-providers.md` (NEW)                                                  | docs        |
| `README.md`                                                                              | MOD (Settings → Transcription section) |

Content of `transcription-providers.md`:
- Existing local providers (`localWhisper`, `parakeet`) — what they do, GPU/CPU support.
- New "Remote HTTPS" provider — how to configure.
- **OpenAI-compatible request/response contract** — the JSON shape any worker must accept/return. This is the page that lets self-hosters implement their own backends. Include a code snippet showing the request body and a sample response.
- Notes on speaker diarization if the worker returns `segments[].speaker`.
- Privacy: "uploads your entire meeting audio to the configured endpoint."

**PR 3 also includes** a sample worker (in a separate `docs/sample-remote-worker/` directory with a `handler.py` template) so upstream reviewers can verify the contract lives up to its claims.

---

# What stays in our fork (NOT PR'd)

These are *not* upstream contributions — they live in our `Hus-Mek/meetily` fork:

1. **`runpod-worker/` directory** — the actual deployment image we run on RTX 3090. Has hardcoded `Mano200600/faster-whisper-large-v2-ar-codeswitching` and uses RunPod-specific `runsync` URL pattern. Stays in our fork.
2. **`docs/runpod-deployment.md`** — our private runbook. In our fork only.
3. **The post-hoc-only restriction** — meetily community may want live streaming later; the upstream `TranscriptionProvider` trait already supports streaming via `Vec<f32>`. We do not gate that. We just choose not to wire the remote runner for live chunks in our fork.
4. **DB columns specifically for "RunPod endpoint ID"** — upstream will use generic `endpoint_url` + `bearer_token`. Our fork may add convenience columns pointing by default at our team's endpoint.

These stay *inside the repository* in our fork but never appear in the upstream-targeted PR branches.

---

# Updated task list (PR-shaped)

| PR | Branch                                              | Tasks (≤ 2-5 min each) |
|----|-----------------------------------------------------|------------------------|
| 0  | issue                                               | Open enhancement issue citing #335 |
| 1  | `feat/pluggable-remote-asr-provider`                | 1.1 add provider.rs module skeleton (no impl); 1.2 `cargo check`; 1.3 write WAV header helper; 1.4 test WAV header; 1.5 implement `WAV-encode`; 1.6 implement `transcribe` against typed request/response; 1.7 happy-path unit test; 1.8 error-path unit test; 1.9 doc comment on contract; 1.10 PR description; 1.11 Open PR |
| 2  | `feat/transcription-provider-remote-ui`             | 2.1 Tauri command `set_remote_config`; 2.2 Tauri command `get_remote_config`; 2.3 unit test for setters; 2.4 Settings UI dropdown extension; 2.5 Settings UI conditional fields; 2.6 "Test connection" button + call; 2.7 i18n strings; 2.8 visual smoke (`pnpm run tauri:dev:cpu`); 2.9 PR description; 2.10 Open PR |
| 3  | `enhance/remote-provider-docs`                      | 3.1 `docs/transcription-providers.md`; 3.2 update README "Settings" section; 3.3 sample `docs/sample-remote-worker/handler.py` (≤50 LoC, pure-Python); 3.4 sample worker smoke test (`python handler.py` against a 5s test file); 3.5 PR description; 3.6 Open PR |

---

# Updated verification checklist

**Pre-PR (per repo):**
- [ ] `cargo check --features=cuda,vulkan` → 0 new warnings
- [ ] `cargo test -p meetily transcription::remote_provider` → all pass
- [ ] `pnpm install && pnpm run tauri:dev:cpu` → app boots, Settings UI shows the new dropdown
- [ ] `pnpm run tauri:build` → release artifact builds
- [ ] Branch is up to date with `origin/devtest` (no merge bubbles)
- [ ] PR description matches `.github/pull_request_template.md` exactly
- [ ] Screenshots of the new Settings UI panel in the PR description

**Upstream-friendly:**
- [ ] PR targets `devtest`, not `main`
- [ ] One conceptual change per PR (no UI changes in PR 1, no logic changes in PR 3)
- [ ] No `runpod-worker/`, no `ManoRashad` strings, no RunPod URLs in upstreamable code

**Out-of-repo:**
- [ ] Our fork (`Hus-Mek/meetily`) consolidates the three merged PRs plus the `runpod-worker/` artifact
- [ ] `docs/runpod-deployment.md` exists in fork only (private org runbook)
- [ ] DB migration for our fork's RunPod-specific columns lives in fork only

---

# Open questions for maintainers (issue-discussion prep)

1. Should the `TranscriptResult.confidence` field get a new sibling `Vec<SegmentAnnotation>` that captures speaker labels, or is it OK to keep using the existing `text` field with a "SPEAKER_00:" prefix convention?  *(We prefer the prefix — backward-compatible, doesn't require touching the trait.)*
2. Does upstream have a preferred CI path for changes that touch `frontend/` (the TS side) but not `src-tauri/`? The PREVIEW workflow appears to gate both.
3. Is there a typed settings table we should reuse instead of a JSON blob?  *(If yes, our plan swaps to it; if no, JSON blob is fine.)*

---

# Plan summary

Three small upstream-PRs (PR 1 = provider impl, PR 2 = UI, PR 3 = docs), each ≤ 250 LoC of upstream-owned code, each targeting `devtest`, each with the conventional commit format and the PR template. **Our fork** keeps the full Path B plan (the original plan file) including the RunPod worker, hardcoded model selection, and org-specific DB columns.

The two plans together mean: we move fast internally with all the vendor-specific pieces, and we share a clean, vendor-neutral abstraction upstream that other orgs can adopt.
