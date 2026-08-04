# Design: Generate OpenSpec artifacts from meeting transcript

## Technical Approach

Implement a new Rust `openspec/` backend module (same shape as `summary/`: `commands.rs` + `service.rs`) plus a frontend button group that mirrors `SummaryGeneratorButtonGroup.tsx`. Flow: frontend click → backend preflight (Node/CLI) → meeting-scoped working dir reset → run OpenSpec CLI with transcript+summary seed → zip `openspec/changes/<slug>/` → return zip path + filename → frontend triggers native Save As.

This maps directly to proposal/spec requirements for state machine, CLI detection, timeout/error distinctions, zip packaging, and overwrite semantics.

## Architecture Decisions

| Decision | Options | Tradeoff | Chosen |
|---|---|---|---|
| Backend command shape | One command (generate+save) vs two commands (`generate`, `save_as`) | Two-step keeps generation deterministic and lets frontend own done transition when Save As starts | **Two commands**: `api_generate_openspec_bundle`, `api_save_openspec_bundle_as` |
| Save dialog implementation | Frontend plugin APIs vs Rust `tauri_plugin_dialog` | Rust path reuses existing plugin initialization and avoids new JS plugin surface | **Rust dialog** via `app.dialog().file().blocking_save_file()` |
| CLI invocation | `npx openspec@latest` only vs global `openspec` fallback | `npx` best freshness; global improves offline/locked-down envs | **Detect global first, else npx** |
| Error transport | String errors vs typed enum | Strings are brittle for UX branching | **Typed enum payload** with stable `code` |
| Regeneration storage | Append versions vs overwrite per meeting | Versioning adds complexity and non-goal | **Overwrite** meeting-scoped workspace |

## Data Flow

`OpenSpecGeneratorButtonGroup` → `useOpenSpecGeneration` → `invoke('api_generate_openspec_bundle')`  
→ `openspec::commands` → `OpenSpecService`:
1) resolve meeting/transcript/summary
2) detect Node + CLI
3) reset workspace `<app_data_dir>/openspec-generation/<meeting_id>/`
4) write seed context files
5) run CLI with timeout and captured stdout/stderr
6) zip `openspec/changes/<slug>/`
7) return `{ zip_temp_path, suggested_filename, slug }`

Frontend on success:
`invoke('api_save_openspec_bundle_as', { zipTempPath, suggestedFilename })` → native Save As → state `done`.

## File Changes

| File | Action | Description |
|---|---|---|
| `frontend/src-tauri/src/openspec/mod.rs` | Create | Module exports + tauri command re-exports (mirror `summary/mod.rs`). |
| `frontend/src-tauri/src/openspec/commands.rs` | Create | Tauri commands, request/response DTOs, state-safe command boundary. |
| `frontend/src-tauri/src/openspec/service.rs` | Create | Node/CLI detection, workspace reset, CLI run, zip creation, error mapping. |
| `frontend/src-tauri/src/lib.rs` | Modify | `pub mod openspec;` and register new commands in `generate_handler!`. |
| `frontend/src/lib/utils.ts` | Modify | Add detector helper for Node/OpenSpec-missing errors (parallel to Ollama helper). |
| `frontend/src/lib/app-i18n.ts` | Modify | Add i18n keys for OpenSpec states/errors/actions. |
| `frontend/src/components/MeetingDetails/OpenSpecGeneratorButtonGroup.tsx` | Create | UI control mirroring Summary generator button group behavior. |
| `frontend/src/hooks/meeting-details/useOpenSpecGeneration.ts` | Create | Idle/generating/error/done state machine + invoke wiring. |
| `frontend/src/app/meeting-details/page-content.tsx` | Modify | Wire new hook and button group with meeting/transcript context. |

## Interfaces / Contracts

```rust
#[derive(Serialize, Deserialize)]
pub struct GenerateOpenSpecInput { pub meeting_id: String }

#[derive(Serialize, Deserialize)]
pub struct GenerateOpenSpecSuccess {
  pub zip_temp_path: String,
  pub suggested_filename: String,
  pub slug: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenSpecErrorCode { NodeMissing, CliMissing, CliFailed, NetworkUnavailable, Timeout, InvalidInput, IoFailure }

#[derive(Serialize, Deserialize)]
pub struct OpenSpecErrorPayload { pub code: OpenSpecErrorCode, pub message: String, pub stderr: Option<String> }
```

Command contract:
- `api_generate_openspec_bundle(meeting_id) -> OpenSpecGenerationResult` (tagged union: success/error)
- `api_save_openspec_bundle_as(zip_temp_path, suggested_filename) -> { saved_path?: String, cancelled: bool }`

## Testing Strategy

| Layer | What to Test | Approach |
|---|---|---|
| Unit (Rust) | CLI detector, timeout mapping, error classification, slug/workspace pathing, zip output contents | Table-driven tests in `openspec/service.rs` with temp dirs + mocked command runner trait. |
| Integration (Rust command) | End-to-end command returns success/error payload shapes; overwrite semantics | Invoke commands against test DB fixture + temp app data directory. |
| UI (React) | Button states (`idle/generating/error/done`) and error toast branching by `code` | Component tests for hook + button group; mocked `invoke`. |
| E2E (desktop smoke) | Generate → Save As appears → zip saved | Manual QA script (v1) because this is first export path. |

## Threat Matrix

| Boundary | Applicability | Design response | Planned RED tests |
|---|---|---|---|
| Documentation-like paths | **Applicable** (process integration/executable boundary) | Never execute meeting-derived paths; execute only resolved `node`, `npx`, or global `openspec` binaries with argument vectors (no shell string interpolation). | Meeting title/transcript containing `README.sh`/`requirements.txt` tokens cannot influence executable selection. |
| Git repository selection | **N/A** (no git invocation) | None | None |
| Commit state | **N/A** (no commit flow) | None | None |
| Push state | **N/A** (no push flow) | None | None |
| PR commands | **N/A** (no PR automation) | None | None |

Safe behavior: bounded subprocess, deterministic cwd, typed failures. Failure behavior: return classified payload and keep UI in `error` with actionable message.

## Migration / Rollout

No data migration required. Rollout as feature-complete path behind existing transcript-availability guard.

## Open Questions

- [ ] Should `npx openspec@latest` be bypassed by policy in offline/corporate environments (global-only mode)?
- [ ] Do we want automatic cleanup of stale temp zip files older than N days?
