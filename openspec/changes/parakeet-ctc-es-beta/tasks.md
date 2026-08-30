# Tasks: Add Parakeet CTC ES as beta downloadable transcription option

## Phase 1: Backend model catalog and runtime validation

- [x] 1.1 Update `frontend/src-tauri/src/parakeet_engine/parakeet_engine.rs` to add a Parakeet CTC ES model entry with artifact metadata, size, expected files, and descriptive status alongside existing TDT variants.
- [x] 1.2 Update `frontend/src-tauri/src/parakeet_engine/commands.rs` so `parakeet_get_available_models`, `parakeet_download_model`, retry/cancel flows, and corrupted-model handling recognize the new CTC ES model string.
- [x] 1.3 Tighten selected-model validation in `frontend/src-tauri/src/parakeet_engine/commands.rs` so explicit CTC ES selection checks the required files for CTC ES rather than treating any downloaded Parakeet model as sufficient.
- [x] 1.4 Update `frontend/src-tauri/src/audio/transcription/engine.rs` so `provider=parakeet` + `model=<ctc-es>` resolves and loads the selected CTC ES variant without silent fallback to TDT.
- [x] 1.5 Add or update Rust tests around Parakeet model inventory and validation to prove CTC ES appears as valid selectable model and fails closed when the selected variant is missing/invalid.

## Phase 2: Frontend beta model UX in Settings

- [x] 2.1 Update `frontend/src/lib/parakeet.ts` with display/config metadata for Parakeet CTC ES, including beta-friendly name, usage description, sizing, and ordering while preserving TDT as recommended/default visual choice.
- [x] 2.2 Update `frontend/src/components/ParakeetModelManager.tsx` to render the CTC ES model card using the existing download/select lifecycle, with clear Beta labeling and usage copy matching the style of the other transcription models.
- [x] 2.3 Update `frontend/src/components/TranscriptSettings.tsx` only as needed to keep Parakeet settings copy coherent with the new beta option, without adding a new provider row.
- [x] 2.4 Update `frontend/src/lib/app-i18n.ts` with localized strings for the new beta model name, usage description, and any beta disclaimer text.
- [x] 2.5 Add or update frontend tests for Parakeet settings/model manager behavior to prove the beta option appears, can be selected, and preserves TDT as the visually primary/default option.

## Phase 3: Persistence and live recording compatibility gate

- [x] 3.1 Verify and, if needed, adjust transcript-config persistence paths so `api_save_transcript_config` continues storing `provider=parakeet` while restoring the explicit CTC ES model selection on reload.
- [x] 3.2 Update `frontend/src/hooks/useRecordingStart.ts` so live-recording readiness checks succeed when the selected beta model is downloaded and valid, while preserving current TDT-default behavior for users who never switched.
- [x] 3.3 Add focused tests for the start-recording readiness logic covering both cases: unchanged TDT default path and explicit CTC ES selected path.
- [x] 3.4 Produce live-path verification evidence showing that a recording session can initialize with Parakeet CTC ES selected and that introducing the beta option does not regress the current default recording path.

## Phase 4: Regression boundaries and verification

- [x] 4.1 Confirm `frontend/src-tauri/src/config.rs`, onboarding context, and onboarding download step remain unchanged in behavior for the first slice.
- [x] 4.2 Run confirmed strict-TDD-aligned backend and frontend test commands once the project test command is explicitly resolved.
- [x] 4.3 Capture verification notes comparing acceptance outcomes against the spec requirements, especially selected-model persistence, no silent migration, and live recording compatibility gate.

## Review Workload Forecast

| Field | Value |
|---|---|
| Estimated changed lines | 300–700 |
| 9000-line budget risk | Low |
| Chained PRs recommended | No |
| Delivery strategy | Single PR |
| Chain strategy | single-pr-default approved |

## Suggested Work Units

| Unit | Goal | Focus |
|---|---|---|
| 1 | Backend model support | catalog, download lifecycle, selected-model validation, no silent fallback |
| 2 | Settings UX | beta card, copy, persistence, preserve TDT primary/default visual path |
| 3 | Live compatibility gate | readiness logic and proof that live recording works with explicit CTC ES selection |
