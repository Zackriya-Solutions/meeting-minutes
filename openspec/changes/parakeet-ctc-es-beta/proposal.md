# Proposal: Add Parakeet CTC ES as beta downloadable transcription option

## Why

Meet4Specs already supports local transcription with Parakeet TDT and local Whisper. For Spanish-heavy interviews, especially LATAM speech with technical English mixed in, the current default Parakeet path is strong but still generic. We want to evaluate a more Spanish-focused realtime option without destabilizing the existing recording flow.

Users should be able to discover, download, and select a **beta** Parakeet CTC ES model from the same transcription settings UX used by the current downloadable models. This gives us a safe way to compare live quality against the current Parakeet TDT baseline before changing defaults.

## What Changes

- Add **Parakeet CTC ES (Beta)** as a downloadable transcription model option in Transcript Settings, following the existing model-card and download UX patterns used by the current local transcription models.
- Keep the current Parakeet TDT option as the default/recommended path; do not replace it in this first slice.
- Store and restore the new model selection through the existing transcript provider/model configuration path.
- Extend local transcription backend model management so the Parakeet provider can validate, download, load, and run the CTC ES variant alongside the existing TDT variants.
- Add acceptance coverage that proves the beta model can be selected and used in a real live-recording path, even though this slice does not change onboarding or default readiness guidance.
- Add product copy that clearly marks the new option as **Beta** and describes its intended use: Spanish-first, LATAM + Spanglish / technical English mixed speech.

## Non-Goals

- Do not change the default transcription provider or default Parakeet model.
- Do not rewrite onboarding to promote the beta model.
- Do not replace the current Parakeet TDT UX, names, or download path.
- Do not add a new cloud service or sidecar server.
- Do not promise diarization, improved colloquial understanding, or benchmark superiority without explicit verification evidence.

## Product Decisions Locked

1. First slice is **Settings only**.
2. New option is shown with a **Beta** badge, not as replacement for current Parakeet TDT.
3. Acceptance prioritizes **LATAM + Spanglish** speech.
4. Success requires a **live quality gate**: the model must be usable in live recording, not only import/retranscription flows.

## Impact

- Affected capability: local transcription model selection and runtime loading for Parakeet-backed models.
- Expected affected areas:
  - `frontend/src/components/TranscriptSettings.tsx`
  - `frontend/src/components/ParakeetModelManager.tsx`
  - `frontend/src/lib/parakeet.ts`
  - `frontend/src/constants/modelDefaults.ts`
  - `frontend/src/lib/app-i18n.ts`
  - `frontend/src-tauri/src/audio/transcription/engine.rs`
  - `frontend/src-tauri/src/audio/transcription/parakeet_provider.rs`
  - `frontend/src-tauri/src/parakeet_engine/*`
  - readiness/check flows that may assume a single Parakeet-ready model exists

## Risks

- Current code appears to treat “Parakeet ready” as a single-state assumption; supporting multiple downloadable Parakeet variants may require separating provider readiness from selected-model readiness.
- The NVIDIA CTC ES path is attractive for Spanish and code-switching, but we should not overclaim wins on modismos without measured evidence.
- Strict TDD is enabled in `openspec/config.yaml`, but the concrete test command is still unconfirmed.

## Acceptance Direction

We consider this proposal successful if users can select a beta Parakeet CTC ES option from Settings using the same style as existing local models, persist that choice, and demonstrate that live recording can initialize and run with that model selected without regressing the current Parakeet TDT default path.
