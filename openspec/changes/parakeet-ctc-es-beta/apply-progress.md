# Apply Progress

## Completed Tasks

- [x] 1.1 Update `frontend/src-tauri/src/parakeet_engine/parakeet_engine.rs` to add a Parakeet CTC ES model entry with artifact metadata, size, expected files, and descriptive status alongside existing TDT variants.
- [x] 1.2 Update `frontend/src-tauri/src/parakeet_engine/commands.rs` so `parakeet_get_available_models`, `parakeet_download_model`, retry/cancel flows, and corrupted-model handling recognize the new CTC ES model string.
- [x] 1.3 Tighten selected-model validation in `frontend/src-tauri/src/parakeet_engine/commands.rs` so explicit CTC ES selection checks the required files for CTC ES rather than treating any downloaded Parakeet model as sufficient.
- [x] 1.4 Update `frontend/src-tauri/src/audio/transcription/engine.rs` behavior through Parakeet validation/load flow so `provider=parakeet` + `model=<ctc-es>` resolves and loads the selected CTC ES variant without silent fallback to TDT.
- [x] 1.5 Add or update Rust tests around Parakeet model inventory and validation to prove CTC ES appears as valid selectable model and fails closed when the selected variant is missing/invalid.
- [x] 2.1 Update `frontend/src/lib/parakeet.ts` with display/config metadata for Parakeet CTC ES, including beta-friendly name, usage description, sizing, and ordering while preserving TDT as recommended/default visual choice.
- [x] 2.2 Update `frontend/src/components/ParakeetModelManager.tsx` to render the CTC ES model card using the existing download/select lifecycle, with clear Beta labeling and usage copy matching the style of the other transcription models.
- [x] 2.3 Update `frontend/src/components/TranscriptSettings.tsx` to keep Parakeet settings copy coherent with the new beta option, without adding a new provider row.
- [x] 2.4 Update `frontend/src/lib/app-i18n.ts` with localized strings for the new beta model name, usage description, and disclaimer text.
- [x] 2.5 Add frontend tests for Parakeet settings/readiness helpers proving the beta option metadata appears, selection ordering preserves TDT as primary, and Parakeet readiness consults selected-model validation.
- [x] 3.1 Verify transcript-config persistence still stores `provider=parakeet` while preserving explicit CTC ES model strings on save/reload paths.
- [x] 3.2 Update `frontend/src/hooks/useRecordingStart.ts` so live-recording readiness checks use selected-model-aware Parakeet validation.
- [x] 3.3 Add focused tests for readiness logic covering unchanged non-Parakeet/default behavior and explicit Parakeet selected-model validation.
- [x] 3.4 Produce live-path verification evidence via automated readiness-path tests showing the live-start gate accepts selected CTC ES and preserves the non-Parakeet/default bypass path.
- [x] 4.1 Confirm `frontend/src-tauri/src/config.rs`, onboarding context, and onboarding download step remain unchanged in behavior for the first slice.
- [x] 4.2 Run confirmed strict-TDD-aligned backend and frontend test commands.
- [x] 4.3 Capture verification notes against the spec requirements.

## Persisted Task Checkbox Updates

- Updated `openspec/changes/parakeet-ctc-es-beta/tasks.md` checkboxes to `- [x]` for tasks 1.1–4.3.

## Files Changed

- `frontend/src-tauri/src/parakeet_engine/commands.rs`
- `frontend/src-tauri/src/parakeet_engine/parakeet_engine.rs`
- `frontend/src/components/ParakeetModelManager.tsx`
- `frontend/src/components/TranscriptSettings.tsx`
- `frontend/src/hooks/useRecordingStart.ts`
- `frontend/src/lib/app-i18n.ts`
- `frontend/src/lib/parakeet.ts`
- `frontend/tests/lib/parakeet-ctc-es.test.ts`
- `frontend/tests/lib/use-recording-start.test.ts`
- `openspec/changes/parakeet-ctc-es-beta/tasks.md`
- `openspec/changes/parakeet-ctc-es-beta/apply-progress.md`

## Test Commands Run

- `cargo test --manifest-path frontend/src-tauri/Cargo.toml resolve_model_to_load -- --nocapture`
- `cargo test --manifest-path frontend/src-tauri/Cargo.toml discover_models_includes_ctc_es_beta_variant -- --nocapture`
- `cd frontend && bun test tests/lib/parakeet-ctc-es.test.ts tests/lib/use-recording-start.test.ts`
- `cargo test --manifest-path frontend/src-tauri/Cargo.toml`
- `cd frontend && bun test`

## Verification Notes

- Backend model discovery now advertises `parakeet-ctc-es-0.6b-int8` alongside existing TDT entries.
- Explicit `provider=parakeet` selections now fail closed when the configured model is missing instead of silently falling back to another downloaded Parakeet model.
- Live recording readiness now calls selected-model-aware Parakeet validation, so explicit CTC ES selection is accepted when downloaded and invalid/missing selections block with the existing setup UX.
- TDT remains the recommended/default visual path; onboarding/default constants were left unchanged.
- Transcript-config persistence path remained compatible because existing save payloads already store `provider=parakeet` plus arbitrary model strings; this slice preserves that contract.
- Full Rust suite hit a pre-existing unrelated failure: `audio::device_detection::tests::test_calculate_buffer_timeout_bluetooth` expected `160ms` and got `159.999996ms`.

## TDD Cycle Evidence

| Task | Test File | Layer | Safety Net | RED | GREEN | TRIANGULATE | REFACTOR |
|------|-----------|-------|------------|-----|-------|-------------|----------|
| 1.1–1.5 | `frontend/src-tauri/src/parakeet_engine/commands.rs`, `frontend/src-tauri/src/parakeet_engine/parakeet_engine.rs` | Unit | N/A (no pre-existing focused Parakeet tests) | ✅ Wrote failing Rust tests for missing helper/model inventory | ✅ Focused cargo tests passed | ✅ explicit selected / missing explicit / default path + inventory coverage | ✅ extracted `resolve_parakeet_model_to_load` and reused validation command |
| 2.1–2.5 | `frontend/tests/lib/parakeet-ctc-es.test.ts` | Unit | N/A (new focused file) | ✅ Wrote failing metadata/ordering test first | ✅ Focused bun test passed | ✅ metadata + ordering cases | ✅ added shared `getParakeetModelSections` helper |
| 3.1–3.4 | `frontend/tests/lib/use-recording-start.test.ts` | Unit | N/A (new focused file) | ✅ Wrote failing readiness-path test first | ✅ Focused bun test passed | ✅ parakeet selected-model path + non-parakeet bypass | ✅ extracted `checkRecordingTranscriptionReady` helper |
| 4.1–4.3 | command evidence + artifact updates | Verification | ✅ focused/full commands run | ➖ behavior-verification task | ✅ evidence captured | ➖ verification-only | ➖ none needed |

## Deviations From Design

- No onboarding/default-path code changes were required; verification only.
- Live-path evidence is automated readiness-path evidence rather than a device-backed manual recording session in this environment.

## Remaining Tasks

- None.

## Workload / PR Boundary

- Single PR implementation slice, within approved single-PR plan.

## Structured Status Consumed

```yaml
schemaName: spec-driven
changeName: parakeet-ctc-es-beta
artifactStore: both
planningHome:
  root: /home/pc/projects/docker/meet4specs
  changesDir: /home/pc/projects/docker/meet4specs/openspec/changes
changeRoot: /home/pc/projects/docker/meet4specs/openspec/changes/parakeet-ctc-es-beta
artifactPaths:
  proposal:
    - openspec/changes/parakeet-ctc-es-beta/proposal.md
  specs:
    - openspec/changes/parakeet-ctc-es-beta/specs/transcription-model-selection/spec.md
  design:
    - openspec/changes/parakeet-ctc-es-beta/design.md
  tasks:
    - openspec/changes/parakeet-ctc-es-beta/tasks.md
  applyProgress:
    - openspec/changes/parakeet-ctc-es-beta/apply-progress.md
  verifyReport: []
  syncReport: []
contextFiles:
  proposal:
    - openspec/changes/parakeet-ctc-es-beta/proposal.md
  specs:
    - openspec/changes/parakeet-ctc-es-beta/specs/transcription-model-selection/spec.md
  design:
    - openspec/changes/parakeet-ctc-es-beta/design.md
  tasks:
    - openspec/changes/parakeet-ctc-es-beta/tasks.md
  applyProgress:
    - openspec/changes/parakeet-ctc-es-beta/apply-progress.md
  verifyReport: []
  syncReport: []
artifacts:
  proposal: done
  specs: done
  design: done
  tasks: done
  applyProgress: done
  verifyReport: missing
  syncReport: missing
taskProgress:
  total: 17
  complete: 17
  remaining: 0
  unchecked: []
deferredParentActions:
  total: 0
  complete: 0
  remaining: 0
  unchecked: []
taskArtifactErrors: []
applyState: all_done
dependencies:
  apply: all_done
  verify: blocked
  sync: blocked
  archive: blocked
actionContext:
  mode: repo-local
  workspaceRoot: /home/pc/projects/docker/meet4specs
  allowedEditRoots:
    - /home/pc/projects/docker/meet4specs
  warnings:
    - Engram reads were partially unavailable; openspec remained authoritative.
nextRecommended: parent-lifecycle
isNonAuthoritative: false
```
