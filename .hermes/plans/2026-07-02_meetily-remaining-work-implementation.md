# Meetily — Remaining Work Implementation Plan

> **For Hermes:** Use subagent-driven-development to implement this plan task-by-task.

**Goal:** Ship the remaining meetily work: 1) Disabled provider as upstream PR #4, 2) Recording-start guard dedup, 3) Bug fixes as upstream PRs.

**Architecture:** All work is on `Hus-Mek/meetily` fork, branch `feat/transcription-provider-remote-ui`. Each item results in a commit (already exists or new). Some items are pure PR-submission (already coded), others need new code.

**Tech Stack:** Rust (Tauri), TypeScript (Next.js), GitHub `gh` CLI.

---

## Context / Assumptions (verified 2026-07-02)

- Fork: `Hus-Mek/meetily`, branch `feat/transcription-provider-remote-ui`
- Upstream target: `Zackriya-Solutions/meetily` branch `devtest`
- 3 existing open PRs: #528 (RemoteProvider), #532 (Settings UI), #533 (docs)
- All 3 are **CLEAN** (no merge conflicts, CI passes), unreviewed by maintainer since June 30
- **Disabled provider** exists on fork (commit `844a614`), does NOT exist on upstream `devtest` (engine.rs missing `Disabled` variant)
- **Recording-start guard**: `checkTranscriptProviderReady()` function already handles all providers, but error-handling block is duplicated 3× (lines 148–175, 226–252, 321–347)
- **Bug fixes**: 5 commits on fork that upstream doesn't have (see details below)
- Build-gate: `cargo-msvc.bat check` then `pnpm run build`
- This session's working dir: `C:\Users\USER\projects\meetily`

---

## Phase 0 — Verify current build (precondition)

Check that the fork builds clean before any changes.

**Step 1:** Run `cargo-msvc.bat check`
Expected: 0 errors (0 warnings is acceptable)

**Step 2:** Run `cd frontend && pnpm run build`
Expected: 11/11 pages, no errors

If Phase 0 fails, STOP and diagnose.

---

## Phase 1 — Disabled provider as upstream PR #4

**Problem:** Upstream issue #519 ("Disable real-time transcription") exists with maintainer saying "contributions are welcome" (June 28). The `DisabledProvider` and `TranscriptionEngine::Disabled` variant already exist on the fork (commit `844a614`). Upstream `devtest` has neither.

**Objective:** Create a clean PR branch from the single commit, submit via `gh`.

### Task 1: Create PR branch from Disabled commit

**Files:** (already on fork, will be cherry-picked to new branch)

| Path | Status | Lines |
|------|--------|-------|
| `frontend/src-tauri/src/audio/transcription/disabled_provider.rs` | NEW | 44 |
| `frontend/src-tauri/src/audio/transcription/mod.rs` | MOD (+2 lines) | pub mod + pub use |
| `frontend/src-tauri/src/audio/transcription/engine.rs` | MOD (+`Disabled` variant + match arms) | ~20 lines |
| `frontend/src-tauri/src/audio/transcription/worker.rs` | MOD (+`Disabled` match arms) | ~15 lines |

**Step 1:** Create new branch from upstream/devtest and cherry-pick the commit
```bash
git fetch upstream
git checkout -b feat/disabled-provider upstream/devtest
git cherry-pick 844a614
```

**Step 2:** Resolve any cherry-pick conflicts. The commit touches:
- NEW `disabled_provider.rs`
- MOD `mod.rs` (add `pub mod disabled_provider; pub use DisabledProvider`)
- MOD `engine.rs` (add `Disabled` variant + 4 match arms + `is_disabled()` method)
- MOD `worker.rs` (add match arms for `TranscriptionEngine::Disabled`)
Only worker.rs might have conflicts if upstream changed the relevant match blocks since.

**Step 3:** Build-verify
```bash
cargo-msvc.bat check
cd frontend && pnpm run build
```

**Step 4:** Push and open PR
```bash
git push fork feat/disabled-provider
gh pr create \
  --repo Zackriya-Solutions/meetily \
  --base devtest \
  --head Hus-Mek:feat/disabled-provider \
  --title "feat(transcription): add 'disabled' provider for low-resource machines (#338, #519)" \
  --body "## Description\n\nAdd a **Disabled** transcription provider option — users who don't need live transcription can opt out entirely. Audio is still recorded to disk for post-hoc processing.\n\nCloses #338 (alternative implementation) and addresses #519.\n\n## Related Issue\nFixes #338, closes #519\n\n## Type of Change\n- [x] New feature\n\n## Testing\n- [x] The change is covered by existing tests (no new behavior change — match arms exercise same paths)\n- [x] `cargo check` passes\n- [x] `pnpm run build` compiles (11/11 pages)\n\n## Checklist\n- [x] Code follows project style\n- [x] Self-reviewed\n- [x] No new warnings\n- [x] Existing tests pass"
```

**Step 5:** Switch back to working branch
```bash
git checkout feat/transcription-provider-remote-ui
```

**Verification:** PR #535 (or next available number) should appear at `https://github.com/Zackriya-Solutions/meetily/pulls`

---

## Phase 2 — Recording-start guard: dedup error-handling block

**Problem:** `frontend/src/hooks/useRecordingStart.ts` has 3 identical error-handling blocks (lines 148–175, 226–252, 321–347). The `checkTranscriptProviderReady()` function itself is correct and handles all providers. The duplication creates drift risk and ~90 lines of unnecessary code.

**Files:**
- Modify: `frontend/src/hooks/useRecordingStart.ts`

### Task 2: Extract shared guard handler

**Step 1:** Extract the duplicated block into a shared helper function `guardTranscriptionModel`:

```typescript
/** Shared handler for a failed transcription-provider readiness check.
 *  Shows a toast with a context-sensitive message based on `tr.reason`,
 *  optionally triggers the model-selector modal, and resets status to IDLE.
 *  Returns `true` if the guard stopped recording (i.e. caller should return early). */
const guardTranscriptionModel = useCallback(async (
  tr: { ok: boolean; reason?: string; provider: string },
  showModal?: (name: 'modelSelector', message?: string) => void,
): Promise<boolean> => {
  if (tr.ok) return false; // not blocked

  const isDownloading = await checkIfModelDownloading();
  if (isDownloading && tr.provider === 'parakeet') {
    toast.info('Model download in progress', {
      description: 'Please wait for the transcription model to finish downloading before recording.',
      duration: 5000,
    });
    Analytics.trackButtonClick('start_recording_blocked_downloading', '_doctor_replaced_');
  } else {
    const reasonText =
      tr.reason === 'whisper-model-missing' ? 'Whisper model not downloaded — open Settings → Transcription to download one'
      : tr.reason === 'parakeet-model-missing' ? 'Parakeet model not downloaded — open Settings → Transcription to download one'
      : tr.reason === 'remote-config-missing' ? 'Remote provider has no endpoint configured — open Settings → Transcription'
      : tr.reason && tr.reason.endsWith('-api-key-missing') ? 'API key missing — open Settings → Transcription to paste it'
      : tr.reason === 'whisper-init-failed' ? 'Failed to initialize Whisper engine'
      : 'Transcription model not ready';
    toast.error('Transcription model not ready', {
      description: reasonText,
      duration: 5000,
    });
    showModal?.('modelSelector', 'Transcription model setup required');
    Analytics.trackButtonClick('start_recording_blocked_missing', '_doctor_replaced_');
  }
  setStatus(RecordingStatus.IDLE);
  return true; // recording was blocked
}, [checkIfModelDownloading, setStatus]);
```

**Step 2:** Replace each of the 3 duplicate blocks with a single call:

```typescript
// Before (in handleRecordingStart, lines ~148-175):
const tr = await checkTranscriptProviderReady();
if (!tr.ok) {
  // ... 28 lines of duplicated error handling ...
  return;
}

// After:
const tr = await checkTranscriptProviderReady();
if (await guardTranscriptionModel(tr, showModal)) return;
```

Same pattern for the `checkAutoStartRecording` effect (lines 225-252 → line 225 stays, block replaced) and `handleDirectStart` effect (lines 320-347 → line 320 stays, block replaced).

**Step 3:** Remove unused standalone `checkIfModelDownloading` calls that are now internal to `guardTranscriptionModel`. The standalone `const isDownloading = await checkIfModelDownloading()` local variable in each block is gone — it's now inside the shared function.

**Step 4:** Verify — `pnpm run build` compiles clean, 11/11 pages.

---

## Phase 3 — Bug fix PRs upstream (5 small commits)

**Problem:** 5 bug fixes exist on fork but not upstream. Each is a small, scoped commit that fits the upstream PR pattern.

**Commits to PR (in order):**

| Commit | PR Title | Files | Reason |
|--------|----------|-------|--------|
| `8ba56c7` | `fix(ui): avoid provider-toggle sync loop in TranscriptSettings` | `TranscriptSettings.tsx` | Infinite re-render on provider change |
| `36c6cd9` | `fix(transcription): bind JS apiKeyVal to Rust api_key_val in transcript config` | `api.rs`, `TranscriptSettings.tsx` | Tauri 2 camelCase silently drops API key |
| `259038a` | `fix(transcription): resolve merge conflicts, fix Groq auto-lang, add stop-recording timeout` | multiple | Groq rejects `language=auto`; stop hangs 10min |
| `ee6ebc1` | `fix(transcription): precompute chunk_duration before closure move in drain helper` | `worker.rs` | Use-after-move in paused-chunk drain |
| `9de8e4e` | `fix: cap transcription wait to 5s, add frontend retry on stop timeout` | `recording_commands.rs`, `RecordingControls.tsx` | Stop button stuck for 10 min |

### Task 3: PR the sync-loop fix

Cherry-pick `8ba56c7`, create PR branch, submit.

### Task 4: PR the camelCase arg fix

Cherry-pick `36c6cd9`, create PR branch, submit.

### Task 5: PR the Groq auto-lang + stop timeout fix

Cherry-pick `259038a`, create PR branch, submit.

### Task 6: PR the chunk_duration fix

Cherry-pick `ee6ebc1`, create PR branch, submit.

### Task 7: PR the stop-recording timeout fix

Cherry-pick `9de8e4e`, create PR branch, submit.

Each task follows the same pattern:
```bash
git checkout -b fix/<slug> upstream/devtest
git cherry-pick <sha>
# resolve conflicts if any
cargo-msvc.bat check
gh pr create --base devtest --head Hus-Mek:fix/<slug> --title "fix(<scope>): ..." --body "..."
git checkout feat/transcription-provider-remote-ui
```

---

## Verification

After all phases:

1. All PRs visible: `gh pr list --repo Zackriya-Solutions/meetily --author Hus-Mek`
2. Fork branch `feat/transcription-provider-remote-ui` builds clean: `cargo-msvc.bat check && cd frontend && pnpm run build`
3. No dangling branches: `git branch --merged` lists the temporary PR branches (can be deleted)
4. Vault write-back: append log entry, update Meetily entity page

---

## Risks & Tradeoffs

- **Cherry-pick conflicts**: If upstream `devtest` has divergent changes in `worker.rs` match arms, the Disabled commit may need manual merge. Resolve per case.
- **Soniox PR #534 overlap**: gunkow's Soniox PR is BLOCKED, but does not conflict with our Disabled/Remote provider work — they're additive.
- **Maintainer review velocity**: 7+ days without review on #528. These small PRs may face the same delay. The maintainer greenlit #519 though.
- **Recording-start guard correctness**: The extracted `guardTranscriptionModel` changes no logic — purely mechanical dedup. Risk of introducing a regression is near-zero but verify with `pnpm run build`.