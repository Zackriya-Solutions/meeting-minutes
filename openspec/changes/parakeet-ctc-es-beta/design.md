# Design: Add Parakeet CTC ES as beta downloadable transcription option

## Technical Approach

Keep the existing `parakeet` provider and extend its model catalog to include a second selectable runtime variant: **Parakeet CTC ES (Beta)**. Reuse the current downloadable-model UX and backend lifecycle instead of introducing a new provider or server. The first slice stays **Settings-only** in product surface, but runtime validation must prove that a user who explicitly selected the beta model can still start a live recording session successfully.

The design goal is to separate:

1. **provider identity**: still `parakeet`
2. **selected model identity**: `parakeet-tdt-0.6b-v3-int8` vs new CTC ES model
3. **default path**: unchanged TDT default for users who never switch
4. **beta path**: explicit opt-in through Settings

This avoids churn in onboarding and analytics while allowing the backend to load a different Parakeet-family asset when the saved transcript config names the beta model.

## Architecture Decisions

| Decision | Options | Tradeoff | Chosen |
|---|---|---|---|
| Exposure model | New provider vs new Parakeet model entry | New provider would touch more UI, config, and engine branching; model entry reuses existing Parakeet flows | **New Parakeet model entry** |
| Product surface | Settings-only vs onboarding/default changes | Settings-only reduces blast radius and honors proposal | **Settings-only** |
| Default selection | Switch default to CTC ES vs preserve TDT | Switching default adds risk and violates first-slice scope | **Preserve current TDT default** |
| Beta signaling | Plain model vs beta-labeled model card | Beta label sets expectation without adding separate provider | **Beta-labeled model card** |
| Live compatibility proof | Settings-only proof vs live recording proof | User explicitly requested live-quality gate | **Require live recording evidence** |
| Readiness semantics | Any Parakeet model present vs selected model ready | Current code appears biased toward a single Parakeet-ready state; that breaks multi-variant support | **Refactor toward selected-model readiness for explicit beta path while preserving default path behavior** |

## Current Integration Shape

### Frontend

- `frontend/src/components/TranscriptSettings.tsx`
  - currently exposes `localWhisper` and `parakeet`
  - should remain provider-level unchanged
  - Parakeet section continues to render `ParakeetModelManager`

- `frontend/src/components/ParakeetModelManager.tsx`
  - already owns model inventory, download progress listeners, selection, and save-through to `api_save_transcript_config`
  - best place to add Beta card copy, recommended ordering, and selection logic for CTC ES

- `frontend/src/lib/parakeet.ts`
  - contains `MODEL_DISPLAY_CONFIG`, `PARAKEET_MODEL_CONFIGS`, recommended model helper, and Tauri command wrappers
  - should become the source of frontend display metadata for the new beta model

- `frontend/src/hooks/useTranscriptionModels.ts`
  - consumes `parakeet_get_available_models`
  - should automatically pick up the new entry once backend inventory includes it

### Backend

- `frontend/src-tauri/src/parakeet_engine/commands.rs`
  - owns `parakeet_get_available_models`, `parakeet_download_model`, `parakeet_validate_model_ready_with_config`, and load/selection lifecycle
  - primary backend command boundary for the new model

- `frontend/src-tauri/src/parakeet_engine/parakeet_engine.rs`
  - appears to own model catalog and ONNX loading details
  - should add the new CTC ES artifact definition and loader mapping here

- `frontend/src-tauri/src/audio/transcription/engine.rs`
  - already resolves transcript config and delegates to Parakeet or Whisper
  - should continue using provider `parakeet`, but the selected model string must drive which Parakeet asset gets validated/loaded

- `frontend/src-tauri/src/api/api.rs`
  - default transcript config remains `provider: "parakeet"` + default model constant
  - no default migration to CTC ES

### Gating / Default-sensitive Areas

- `frontend/src/hooks/useRecordingStart.ts`
  - currently checks Parakeet readiness before live recording
  - must not fail when the user explicitly selected the beta model and that model is downloaded/valid
  - must continue current behavior for users still on TDT default

- `frontend/src/contexts/OnboardingContext.tsx`
- `frontend/src/components/onboarding/steps/DownloadProgressStep.tsx`
  - currently hardcode `parakeet-tdt-0.6b-v3-int8`
  - should stay unchanged in first slice

## Data Flow

### Settings path

`TranscriptSettings` → `ParakeetModelManager` → `api_save_transcript_config(provider='parakeet', model='<selected>')`

No provider-level schema change is required. The selected model string continues to be the switch.

### Download path

`ParakeetModelManager.downloadModel(modelName)` → `ParakeetAPI.downloadModel(modelName)` → `parakeet_download_model`
→ backend catalog resolves artifact URLs / expected files / download states → existing progress events

Reused events:

- `parakeet-model-download-progress`
- `parakeet-model-download-complete`
- `parakeet-model-download-error`

### Live recording path

`useRecordingStart` → `checkParakeetReady()` / inventory lookup → backend `parakeet_validate_model_ready_with_config` → `get_or_init_transcription_engine()` → load selected Parakeet variant → live transcription worker

Critical design point: readiness must answer **"is selected Parakeet model ready?"** for explicit beta users, not merely **"does any Parakeet asset exist?"**.

## File Changes

| File | Action | Description |
|---|---|---|
| `frontend/src/lib/parakeet.ts` | Modify | Add frontend metadata for Parakeet CTC ES: friendly name, beta badge/tagline, size, intended usage description, recommended ordering rules. |
| `frontend/src/components/ParakeetModelManager.tsx` | Modify | Render CTC ES card with Beta styling/copy, preserve TDT as recommended/default visual choice, reuse existing download/select listeners. |
| `frontend/src/components/TranscriptSettings.tsx` | Minimal modify | No new provider; only ensure Parakeet section copy remains coherent with the new beta option. |
| `frontend/src/lib/app-i18n.ts` | Modify | Add localized strings for CTC ES beta name, usage description, and any beta disclaimer copy. |
| `frontend/src/constants/modelDefaults.ts` | Preserve or minimal annotate | Keep TDT default unchanged; optional comments/docs only. |
| `frontend/src-tauri/src/parakeet_engine/parakeet_engine.rs` | Modify | Add CTC ES model catalog entry, artifact metadata, file validation rules, and loader mapping. |
| `frontend/src-tauri/src/parakeet_engine/commands.rs` | Modify | Ensure inventory, validation, download, retry, and load flows accept the new model string. |
| `frontend/src-tauri/src/audio/transcription/engine.rs` | Modify | Preserve provider branching, but make selected-model handling robust when `provider == parakeet` and model is CTC ES. |
| `frontend/src/hooks/useRecordingStart.ts` | Modify | Make readiness check compatible with selected beta model for live recording without changing onboarding/default behavior. |
| `frontend/src-tauri/src/config.rs` | Preserve | Default Parakeet model remains `parakeet-tdt-0.6b-v3-int8`. |
| `frontend/src/contexts/OnboardingContext.tsx` | No change expected | Preserve TDT onboarding path in first slice. |
| `frontend/src/components/onboarding/steps/DownloadProgressStep.tsx` | No change expected | Preserve existing onboarding download flow in first slice. |

## UX Notes

- CTC ES appears inside existing Parakeet model list, not as a new provider row.
- TDT remains visually primary: “recommended” / first card.
- CTC ES card copy should match existing model description style, but include:
  - **Beta**
  - intended for **Spanish-first, LATAM + Spanglish / technical English mixed speech**
- No default auto-selection after introduction.

## Runtime / Model Management Notes

- Backend must support separate artifact identity for CTC ES alongside TDT variants.
- Validation must check the files required by the selected model, not just any Parakeet folder.
- Model loading must be deterministic from saved config string.
- Fallback to another Parakeet model silently is forbidden when the user explicitly selected CTC ES.

## Testing Strategy

| Layer | What to Test | Approach |
|---|---|---|
| Frontend unit/component | Beta card visibility, label/copy, select/persist behavior, TDT remains recommended/default visual choice | React tests around `ParakeetModelManager` / settings interactions with mocked invoke/listen |
| Backend unit | CTC ES appears in available models, validates as legal selection, selected-model validation does not silently accept wrong variant | Rust tests in `parakeet_engine` command/model layers |
| Integration | Saved transcript config with `provider=parakeet` + `model=<ctc-es>` leads to CTC ES validation/load path | Rust integration tests around config + engine init |
| Live path proof | Explicit evidence that live recording can start with CTC ES selected and does not regress TDT default path | Focused runtime proof / integration test or documented manual validation if automation is not feasible yet |

## Threats / Risks

| Risk | Why it matters | Design response |
|---|---|---|
| Single readiness state | Current code appears to treat Parakeet as one ready/not-ready bucket | Move readiness checks toward selected-model awareness for explicit beta path |
| Silent fallback | User could think CTC ES is active while TDT actually runs | Validation/load flow must reject invalid selected-model state rather than silently switching |
| Default-path regression | Onboarding and existing users rely on current TDT assumptions | Keep defaults/constants/onboarding untouched in first slice |
| Overclaiming Spanish quality | Product goal includes modismos/Spanglish, but not yet proven | Keep Beta framing and require live-path evidence, not marketing claims |
| Strict TDD command unknown | Apply/verify phases need real test command | Confirm exact test command before implementation phase |

## Rollout

- Ship as **beta option** only.
- Do not modify onboarding or migrate defaults.
- Reassess after live evidence and comparative evaluation against current TDT path.
