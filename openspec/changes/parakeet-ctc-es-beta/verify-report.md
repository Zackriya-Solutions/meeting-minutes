# Verify Report: parakeet-ctc-es-beta

## Outcome

**Status: FAIL**

Two CRITICAL blockers prevent PASS:

1. The backend does not define a distinct CTC ES download artifact; `parakeet-ctc-es-0.6b-int8` currently reuses the default non-`-v2-` Parakeet TDT v3 download URL and file-size assumptions.
2. The required live-recording compatibility evidence is not present; the added test only verifies a mocked readiness-command path, not actual CTC ES initialization in the live recording flow.

---

## Structured status and actionContext

- `changeName`: `parakeet-ctc-es-beta`
- `artifactStore`: `both` (`openspec/` present; authoritative on disk)
- `applyState`: `all_done`
- `taskProgress`: `17/17 complete`, `0 remaining`
- `actionContext.mode`: `repo-local`
- `workspaceRoot`: `/home/pc/projects/docker/meet4specs`
- `allowedEditRoots`: `/home/pc/projects/docker/meet4specs`
- Change selection was explicit and unambiguous.
- Artifact availability used for verification: proposal/spec/design/tasks/apply-progress all present.

---

## Task completion status

- Unchecked implementation tasks matching `^\s*- \[ \]`: **none**
- Tasks file is fully checked, but verification found checked tasks that are not fully substantiated:
  - **3.4** `Produce live-path verification evidence showing that a recording session can initialize with Parakeet CTC ES selected...` → **not proven by current evidence**
  - **4.3** `Capture verification notes against the spec requirements` → notes exist, but they overstate backend/live-path completion

---

## Spec coverage

| Requirement | Result | Evidence |
|---|---|---|
| Parakeet CTC ES Beta Option In Settings | PASS | Frontend metadata/copy added in `frontend/src/lib/parakeet.ts:41-55`, `frontend/src/components/TranscriptSettings.tsx:166-175`, and translated strings in `frontend/src/lib/app-i18n.ts`. |
| Selected Beta Model Persists Across Sessions | PASS with limited direct test evidence | Selection still saves through `api_save_transcript_config` and provider/model contract is unchanged; no explicit restart-level automated test was added. |
| Backend Model Lifecycle Supports Parakeet CTC ES | **FAIL (CRITICAL)** | Inventory lists the model in `frontend/src-tauri/src/parakeet_engine/parakeet_engine.rs:174-177`, but download logic still routes every non-`-v2-` model to the default v3 URL in `frontend/src-tauri/src/parakeet_engine/parakeet_engine.rs:594-600` and uses default v3 Int8 size assumptions in `:649-667`. This does not prove a distinct CTC ES artifact can be downloaded/validated/loaded. |
| Live Recording Compatibility Gate | **FAIL (CRITICAL)** | `frontend/tests/lib/use-recording-start.test.ts:6-29` only mocks `parakeet_validate_model_ready` and asserts command order. It does not prove actual engine init/load with CTC ES selected, nor a live recording start. |
| Onboarding And Default Path Stay Unchanged In First Slice | PASS | TDT default preservation is enforced in `frontend/src-tauri/src/parakeet_engine/commands.rs:62-67` and UI ordering in `frontend/src/lib/parakeet.ts:125-144`; no onboarding files were changed. |

---

## Changed-files review

### Confirmed good changes

- Explicit selected-model fail-closed behavior added in `frontend/src-tauri/src/parakeet_engine/commands.rs:40-75`.
- Config-aware Parakeet validation/reload path added in `frontend/src-tauri/src/parakeet_engine/commands.rs:224-302`.
- TDT remains visually primary while CTC ES is shown as beta in frontend metadata/order/copy.
- Recording-start readiness now consults the backend validation path for Parakeet selections in `frontend/src/hooks/useRecordingStart.ts:18-29`.

### Critical findings

1. **Distinct CTC ES artifact not implemented**
   - `parakeet-ctc-es-0.6b-int8` is only added to the catalog.
   - Download source selection is still binary: `-v2-` gets v2 URL, everything else gets the v3 TDT URL.
   - Result: selecting/downloading “CTC ES” can still fetch TDT v3 assets under a different folder name.

2. **Live-path proof is insufficient**
   - The spec/design require proof that the live recording path initializes successfully with CTC ES selected.
   - Current evidence is a unit test with mocked invoke responses; it never exercises real backend model discovery, selected-model loading, engine init, or recording startup.

---

## Test / validation commands

### Focused commands run

- `cargo test --manifest-path frontend/src-tauri/Cargo.toml resolve_model_to_load -- --nocapture` ✅
- `cargo test --manifest-path frontend/src-tauri/Cargo.toml discover_models_includes_ctc_es_beta_variant -- --nocapture` ✅
- `cd frontend && bun test tests/lib/parakeet-ctc-es.test.ts tests/lib/use-recording-start.test.ts` ✅

### Full commands run

- `cargo test --manifest-path frontend/src-tauri/Cargo.toml` ⚠️ failed with known unrelated pre-existing failure:
  - `audio::device_detection::tests::test_calculate_buffer_timeout_bluetooth`
  - observed: `159.999996ms`
  - expected: `160ms`
- `cd frontend && bun test` ✅ (`19 pass, 0 fail`)

### Quality metrics

- `cd frontend && bunx tsc --noEmit` ⚠️ failed in unrelated existing test file:
  - `tests/lib/blocknote-markdown.test.ts(6,10): error TS2339: Property 'restore' does not exist on type '(...args: any[]) => any'.`
  - No changed-file type error was surfaced.
- `cd frontend && bun run lint` ➖ not usable as a non-interactive verification command because `next lint` prompted for initial ESLint setup instead of running checks.
- Coverage analysis skipped — no coverage tool/configured coverage command was detected.

---

## Strict TDD compliance

Strict TDD mode is active (`openspec/config.yaml`) and global verify guidance was loaded.

### TDD Compliance

| Check | Result | Details |
|---|---|---|
| TDD Evidence reported | ✅ | `apply-progress.md` contains a `TDD Cycle Evidence` table. |
| Test files referenced exist | ✅ | `frontend/tests/lib/parakeet-ctc-es.test.ts`, `frontend/tests/lib/use-recording-start.test.ts`, plus inline Rust tests in changed `.rs` files exist. |
| GREEN confirmed by execution | ⚠️ | Focused tests pass, but task `3.1–3.4` evidence does not prove the claimed live-path behavior. |
| Triangulation adequate | ⚠️ | Metadata/order and readiness-path cases were triangulated, but no real live-init case covers the acceptance gate. |
| Safety net for modified files | ⚠️ | Rust row reports `N/A` because no prior focused tests existed, but it modified existing production files. |

**Strict TDD verdict: FAIL** due to incomplete evidence for the live compatibility gate under a strict-TDD change.

### Test layer distribution

| Layer | Tests | Files |
|---|---:|---:|
| Unit | 8 | 4 |
| Integration | 0 | 0 |
| E2E / device-backed live | 0 | 0 |
| Total | 8 | 4 |

### Assertion quality

- No tautologies, ghost loops, smoke-only assertions, or CSS-detail assertions were found in the added TS tests.
- No CRITICAL assertion-quality issue found.
- Minor caution: `frontend/tests/lib/use-recording-start.test.ts:25-29` asserts exact command sequence, which is somewhat implementation-coupled, but it is paired with a behavioral result assertion and is not a blocker.

---

## Review workload / PR boundary

- Approximate changed-line footprint from targeted diff stat: within the forecasted `300–700` range.
- `Chained PRs recommended`: No
- `Delivery strategy`: Single PR
- `Chain strategy`: `single-pr-default approved`
- Boundary assessment: implementation stayed within the assigned slice/files.
- Scope-creep finding: none.

---

## Blockers

### CRITICAL

1. **Backend lifecycle requirement not met**
   - File: `frontend/src-tauri/src/parakeet_engine/parakeet_engine.rs:594-600, 649-667`
   - Problem: CTC ES does not have distinct artifact source/size handling; non-`-v2-` Int8 models still use the TDT v3 download path and assumptions.

2. **Live compatibility completion gate not met**
   - File: `frontend/tests/lib/use-recording-start.test.ts:6-29`
   - Problem: test uses mocked invoke responses and does not prove actual CTC ES initialization/load in a live recording path.

### WARNING

1. Persistence requirement relies on existing save/restore contract and code review; no new restart-level automated test was added.
2. Full Rust suite still has the known unrelated bluetooth timeout failure.
3. Typecheck command reports an unrelated existing frontend test typing issue.

---

## Executive summary

The UI/persistence/default-path parts are mostly in place, and the selected-model fail-closed logic is a real improvement. However, verification fails because the backend still does not implement a distinct CTC ES artifact lifecycle, and the strict acceptance gate requiring live-path evidence is not actually met by the provided tests.
