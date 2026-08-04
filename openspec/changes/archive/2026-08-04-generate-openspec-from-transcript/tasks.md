# Tasks: Generate OpenSpec artifacts from meeting transcript

## Phase 1: Foundation (Rust module + contracts)

- [x] 1.1 Create `frontend/src-tauri/src/openspec/mod.rs` and re-export commands/DTOs like `summary/mod.rs`.
- [x] 1.2 Update `frontend/src-tauri/Cargo.toml`: add `walkdir`; keep existing `zip`; no new npm dependency needed.
- [x] 1.3 RED test in `frontend/src-tauri/src/openspec/service.rs`: meeting text (`README.sh`, `requirements.txt`) cannot affect executable selection.
- [x] 1.4 Add typed envelope in `openspec/service.rs`: `GenerateOpenSpecSuccess`, `OpenSpecErrorCode`, `OpenSpecErrorPayload`, result union.

## Phase 2: Core backend (detect → run → zip)

- [x] 2.1 Implement detection in `openspec/service.rs`: global `openspec` first, fallback `npx openspec@latest`; map `NodeMissing`/`CliMissing`.
- [x] 2.2 Implement overwrite workspace reset in `openspec/service.rs`: `<app_data_dir>/openspec-generation/<meeting_id>/` + seed transcript/summary files.
- [x] 2.3 Implement subprocess runner with timeout and stdout/stderr capture; classify `CliFailed`, `NetworkUnavailable`, `Timeout`, `IoFailure`.
- [x] 2.4 Implement zip creation for `openspec/changes/<slug>/` and return `{zip_temp_path,suggested_filename,slug}`.
- [x] 2.5 Create `frontend/src-tauri/src/openspec/commands.rs`: `api_generate_openspec_bundle` + `api_save_openspec_bundle_as` using `app.dialog().file().blocking_save_file()`.
- [x] 2.6 Register module and commands in `frontend/src-tauri/src/lib.rs` (`pub mod openspec;` + `generate_handler!`).

## Phase 3: Frontend wiring (button group + state machine)

- [x] 3.1 Add OpenSpec error helpers in `frontend/src/lib/utils.ts` (node missing / cli / network / timeout) for UI branching.
- [x] 3.2 Add OpenSpec i18n keys in `frontend/src/lib/app-i18n.ts` for idle/generating/error/done, regenerate, install/download actions.
- [x] 3.3 Create `frontend/src/hooks/meeting-details/useOpenSpecGeneration.ts` state machine: idle→generating→done/error, retry resets error.
- [x] 3.4 Create `frontend/src/components/MeetingDetails/OpenSpecGeneratorButtonGroup.tsx` reusing Summary + Ollama-not-installed UX pattern.
- [x] 3.5 Wire hook/button in `frontend/src/components/MeetingDetails/SummaryPanel.tsx` and `frontend/src/app/meeting-details/page-content.tsx`.

## Phase 4: Tests / verification

- [x] 4.1 Rust tests in `frontend/src-tauri/src/openspec/service.rs`: detection matrix and subprocess error classification.
- [x] 4.2 Rust tests in `frontend/src-tauri/src/openspec/service.rs`: overwrite semantics and zip output contents.
- [x] 4.3 Frontend tests in `frontend/tests/meeting-details/use-openspec-generation.test.ts`: `idle→generating→done` and `generating→error→idle`.
- [x] 4.4 Execute focused checks: `cd frontend/src-tauri && cargo test openspec` and `cd frontend && bun test frontend/tests/meeting-details/use-openspec-generation.test.ts`.
- [x] 4.5 Add one real subprocess timeout-abort runtime test for `SystemCommandRunner` (non-mock path) proving typed `Timeout` and bounded completion.

## Review Workload Forecast

| Field | Value |
|---|---|
| Estimated changed lines | 700–1000 |
| 400-line budget risk | High |
| Chained PRs recommended | Yes |
| Suggested split | PR 1 contracts/detection → PR 2 execution/zip/commands → PR 3 frontend/tests |
| Delivery strategy | ask-on-risk |
| Chain strategy | pending |

Decision needed before apply: Yes
Chained PRs recommended: Yes
Chain strategy: pending
400-line budget risk: High

### Suggested Work Units

| Unit | Goal | Likely PR | Focused test command | Runtime harness | Rollback boundary |
|---|---|---|---|---|---|
| 1 | Contracts + detection + RED security test | PR 1 | `cd frontend/src-tauri && cargo test openspec::service::tests` | `cd frontend/src-tauri && cargo check` | `src-tauri/src/openspec/*`, `src-tauri/Cargo.toml` |
| 2 | CLI run, timeout/error map, zip, Tauri commands | PR 2 | `cd frontend/src-tauri && cargo test openspec` | invoke `api_generate_openspec_bundle` on fixture meeting | `src-tauri/src/openspec/*`, `src-tauri/src/lib.rs` |
| 3 | Hook/button wiring + UI tests | PR 3 | `cd frontend && bun test frontend/tests/meeting-details/use-openspec-generation.test.ts` | manual flow: Generate → Save As dialog | `src/hooks/meeting-details/*`, `src/components/MeetingDetails/*`, `page-content.tsx`, `app-i18n.ts`, `utils.ts` |
