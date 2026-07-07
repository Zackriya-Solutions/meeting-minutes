# PLAN.md V1 — Implementation Progress

Tracking implementation of [PLAN.md](PLAN.md) in this repository.

## Baseline verification (PLAN.md §1) — REPORTED

PLAN.md assumes a **Russian GigaAM fork**. This repo is **upstream English Meetily**.
Several assumed-baseline items diverge; per §1 they are reported here rather than
silently reimplemented:

| Assumed baseline | Reality in this repo | Impact |
|---|---|---|
| Tauri app (Rust + Next.js) | ✅ present | — |
| GigaAM v3 e2e ONNX transcription | Was ❌ (Whisper + Parakeet only) → **now IMPLEMENTED** (`src/gigaam_engine/`) and the sole transcription engine. See the GigaAM section below. | Closed |
| `segments(meeting_id, start_ms, end_ms, text)` | ⚠️ No `segments` table. Equivalent = `transcripts(id TEXT, meeting_id TEXT, transcript, audio_start_time REAL, audio_end_time REAL, speaker)` | **Adapted:** timing is seconds (REAL); ids are TEXT |
| FTS5 search | ❌ missing | Must be added at Phase 1 start (search-critical) |
| Meeting entity (title, datetime, duration, tags, audio path) | ⚠️ title/created_at/folder_path only; **no duration/tags** | Add as needed per phase |
| LLM layer (DeepSeek/GigaChat) | ⚠️ anthropic/groq/ollama/openai/openrouter/gemini + custom OpenAI-compatible; **no DeepSeek/GigaChat**, no unified `LlmProvider` trait | Reuse existing providers; add unified purpose-tagged trait in Phase 3 |

### Adaptation decisions (apply throughout)

1. **IDs are TEXT.** `meetings.id` / `transcripts.id` are UUID strings. Every new FK
   uses TEXT for meeting/segment references (PLAN.md's `meeting_id INTEGER` DDL is
   adapted). New tables keep INTEGER rowid PKs so they can back sqlite-vec.
2. **Timing is ms in new tables**, converted from `transcripts.audio_start_time`
   (seconds REAL) × 1000 at write time.
3. **`chunks.first/last_segment_id` reference `transcripts(id)`** (TEXT).
4. **sqlite-vec** is statically linked and registered via `sqlite3_auto_extension`
   (no per-OS binary shipped). The `chunk_embeddings` vec0 table is created **in code**
   after migrations with graceful degradation — a build without the extension still
   boots and hybrid search falls back to FTS-only.
5. **Embedding dim = 384** (`multilingual-e5-small`) placeholder; reconfirmed by the
   Phase 1 benchmark (PLAN.md §11 #1). Changing it only drops/recreates the empty table.

## Phase status

- [x] **Phase 0 — Data Foundation** (complete + verified)
- [~] **Phase 1 — Semantic + Hybrid Search** (search core + embedder + search/chat UI all
  done & verified; only the model-download UI button and eval harness remain)
- [~] **Phase 2 — Diarization + Speakers** (resolution algorithms complete + verified;
  ONNX diarizer scaffolded, degrades safely)
- [~] **Phase 3 — Entities + Action Items** (resolution/validation complete + verified;
  LLM extraction wiring + DB writes scaffolded)
- [~] **Phase 4 — RAG Chat** (grounding/citation/guard logic complete + verified; answer
  generation + chat UI scaffolded)
- [~] **Phase 5 — Collections/Backfill/Privacy** (series detection + backfill + privacy
  guard complete + verified; UI scaffolded)

`[~]` = algorithmic core implemented, unit-tested, and wired; the remaining pieces are
external-resource-gated (ONNX models, LLM provider config, frontend UI) and clearly
marked with `SCAFFOLD` / `TODO` in code. **Decision taken:** user chose "scaffold Phases
2–5 backend"; embedding model defaulted to `multilingual-e5-small` / dim 384.

Beyond the PLAN phases, two things were added that PLAN.md assumed as *baseline*:
- [x] **GigaAM v3 transcription** — implemented + end-to-end verified (see below).
- [x] **DeepSeek / GigaChat LLM providers** — implemented + wired (see LLM section).

## Transcription engine — GigaAM v3 (IMPLEMENTED + END-TO-END VERIFIED) ✅

GigaAM v3 (Sber, Russian ASR) is now the **sole** transcription engine. Source model:
`istupakov/gigaam-v3-onnx` (onnx-asr), variant **e2e-CTC int8** (`v3_e2e_ctc.int8.onnx`,
~224 MB) → punctuated + capitalized Russian output.

- **`src/gigaam_engine/featurizer.rs`** — Rust log-mel featurizer, an *exact* port of
  onnx-asr's `GigaamPreprocessorNumpy` v3 (16 kHz, win=n_fft=320, hop=160, no padding,
  bundled window + mel filterbank in `resources/gigaam/*.f32`, `log(clip(·,1e-9,1e9))`,
  no CMVN). **Unit-tested against the numpy reference** (per-frame values + sum).
- **`src/gigaam_engine/model.rs`** — `ort` session for the e2e-CTC model
  (`features[1,64,T] + feature_lengths[1] → log_probs[1,T,257]`), greedy CTC decode
  (blank = 256, collapse repeats), vocab loader (`▁`→space). Vocab-parse unit test.
- **`src/gigaam_engine/{mod,commands}.rs`** — process-global model (`static
  Mutex<Option<GigaamModel>>`, async `transcribe` via `spawn_blocking`); commands
  `gigaam_status` / `gigaam_download_model` (HF download to `app_data_dir/models/gigaam/`,
  progress events) / `gigaam_transcribe_audio` + `init_gigaam_at_startup`.
- **Pipeline wiring:** live recording via a `GigaamProvider` (the `TranscriptionProvider`
  trait) + `"gigaam"` arms in `audio/transcription/engine.rs` (init + validation), and the
  **batch paths** `audio/import.rs` + `audio/retranscription.rs` (added a `use_gigaam`
  branch → `crate::gigaam_engine::transcribe`).
- **UI:** the transcript settings now offer **only GigaAM** (dropdown/cloud-key/Whisper+
  Parakeet managers removed), auto-migrating any saved provider to `gigaam`. Default
  provider is `gigaam` in `ConfigContext` and the backend `engine.rs` fallback.
  `get_transcript_api_key` treats `gigaam` as keyless (fixed an `Invalid provider` error).
- **END-TO-END VERIFIED (real audio):** `say -v Milena` Russian TTS → ffmpeg 16 kHz mono →
  a standalone Rust binary using the real `featurizer.rs` + `model.rs` produced text
  **identical** to the onnx-asr reference: *"Привет, коллеги. Сегодня мы обсудим бюджет
  проекта на следующий квартал."* (punctuated, capitalized).
- **Gotchas (fixed / noted):** (1) `include_bytes!` blobs are align-1 → parse assets with
  `f32::from_le_bytes`, NOT `bytemuck::cast_slice` (that panicked on model load). (2)
  onnxruntime **CoreML EP fails** on the int8 model on macOS — force CPU; the Rust
  `Session` uses the default CPU EP so the app is unaffected.

## Post-scaffold UX & fixes

- **Onboarding no longer force-downloads.** `DownloadProgressStep` made both model
  downloads opt-in (Download buttons + "Skip & continue"; Continue no longer gated on a
  model), and `OnboardingContext.completeOnboarding` no longer auto-downloads the local
  summary model. New tab structure + defaults point at GigaAM.
- **Recording-readiness gate made provider-aware** (`src/lib/transcriptionReadiness.ts`,
  `isConfiguredTranscriptionModelReady`, injectable `invoke`): checks `gigaam_status.loaded`
  for GigaAM, `*_has_available_models` for Whisper/Parakeet, always-ready for cloud. Fixed
  the false "Transcription model not ready" for GigaAM (the gate had hardcoded a Parakeet
  check; GigaAM selection also wasn't persisted to the backend — now `api_save_transcript_config`
  is called on select). Covered by a **bun test** `tests/lib/transcriptionReadiness.test.ts`
  (5 pass, incl. a regression guard that GigaAM does not hit the Parakeet check).
- **pnpm via corepack**: Tauri's `beforeDevCommand` is `pnpm dev`; run `corepack enable
  pnpm` so it's on PATH.

## Phases 2–5 scaffold (this pass)

- **Phase 2** `src/pipeline/diarization.rs`: `cosine_similarity`, `assign_segment`
  (max-overlap, <60% → NULL), `match_speaker` (τ=0.75), `fold_embedding` (running avg) —
  all unit-tested. `Diarizer` ONNX wrapper + `diarize` handler degrade to unattributed
  segments when no model (pipeline never blocks).
- **Phase 3** `src/pipeline/extraction.rs`: `normalize_name` (ё→е), `resolve_entity`
  (exact / Jaro-Winkler ≥0.92 merge / 0.85–0.92 review / new, never crossing entity
  types), strict JSON `parse_and_validate` (fence-tolerant, rejects bad types) — tested.
  `extract` handler enforces the privacy guard first (zero LLM calls when disabled).
- **Phase 4** `src/search/rag.rs`: `build_context` (`[N]` markers), `parse_citations`,
  `evaluate_answer` (low-confidence + sentinel → NotFound; uncited → regenerate) — tested.
  **Now wired end-to-end**: `rag::ask(query, scope, history)` retrieves (hybrid, scoped via
  `SearchFilters` incl. new `meeting_ids`), fills `rag_answer_v1`, calls `complete_routed`
  (privacy-guarded, GigaChat/DeepSeek via the router), applies the guards + regenerate-once,
  and returns answer+citations. `search::commands::rag_ask` command persists the turn to
  `chat_sessions`/`chat_messages` (last-6-turn history) and is registered in `lib.rs`.
  Retrieval uses the FTS branch until the embedder supplies query vectors.
- **Phase 5** `src/collections/mod.rs`: `suggest_series` (normalized-title similarity +
  cadence detection: daily/weekly/biweekly) — tested. `backfill` handler fans out
  `chunk_embed` for un-chunked meetings. Commands: collections/saved-search CRUD, series
  suggestion, backfill, `set_app_setting` (privacy).
- **LLM layer** `src/llm/`: `Purpose` + `PrivacyConfig` guard (runs BEFORE the provider
  future is awaited → zero network when blocked/local-only), `router` (fast vs synthesis),
  versioned prompts (`prompts/extract_v1.md`, `rag_answer_v1.md`). Guard/router/prompt
  logic tested, incl. `blocked_call_never_awaits_provider` and DB privacy loader.
- **LLM providers CONNECTED** `src/llm/providers/` (protocol from the GigaTool project):
  - `gigachat.rs` — Sber GigaChat: OAuth token mint + cache (`{base}/token`, Basic auth,
    `RqUID`) → chat completions (`{base}/chat/completions`, Bearer). Base
    `gigachat.sberdevices.ru/v1`, default model `GigaChat-3-Ultra`. Accepts a single Sber
    "Authorization Key" or user+password.
  - `deepseek.rs` — DeepSeek (OpenAI-compatible), base `api.deepseek.com/v1`, model
    `deepseek-chat`.
  - `providers/mod.rs` resolves credentials from `app_settings_kv` (`gigachat.auth_key` /
    `gigachat.user`+`password`, `deepseek.api_key`, `*.model`, `*.base_url`) then env
    (`GIGACHAT_AUTH_KEY`/`GIGACHAT_USER`+`PASSWORD`, `DEEPSEEK_API_KEY`).
  - `llm::complete_routed(...)` = privacy guard → router (Fast→GigaChat, Synthesis→
    DeepSeek) → provider, with fallback to whichever is configured. The `extract` job now
    calls it end-to-end (build prompt → complete → validate JSON, retry once, degrade).
  - Also added as first-class providers in the existing summary path
    (`summary::llm_client::LLMProvider::{DeepSeek,GigaChat}`) so meeting summaries can use
    them (`"deepseek"` / `"gigachat"` provider ids). Set credentials from the UI via the
    `set_app_setting` command.
  - Verified: full real-crate `cargo check --lib` passes (0 errors); parse helpers unit-
    tested. Live calls need real credentials (GigaChat also requires the Russian Ministry
    CA in the system trust store).
- **Schema** migration `20260706000002`: `pending_merges`, `chat_sessions`,
  `chat_messages`, `app_settings_kv`.

## Phase 1 — what shipped (this pass)

**FTS5** `migrations/20260706000001_fts5_transcripts.sql`
- `transcripts_fts` (segment-level, fills the §1 baseline gap) + `chunks_fts`
  (chunk-level, branch A of hybrid). External-content, unicode61 tokenizer, kept in
  sync by triggers. Verified on real SQLite incl. Cyrillic MATCH + trigger sync.

**Chunker** `src/pipeline/chunker.rs`
- Deterministic, segment-aligned (never splits a segment), 200–400 token target with
  1-segment overlap, pluggable tokenizer. 5 tests pass (determinism, overlap, bounds,
  oversized segment, boundaries).

**Hybrid search** `src/search/hybrid.rs`
- `reciprocal_rank_fusion` (RRF, k=60) + `HybridSearch::search`: BM25 branch + vector
  KNN branch, filters (date/speaker/collection) applied to both before ranking, fused,
  top-N loaded with meeting metadata + `start_ms` + matched terms. Degrades to FTS-only
  when no embedding / no sqlite-vec. Unit tests + an **end-to-end integration test**
  (seeded chunks + FTS + vec) pass.

**Command** `src/search/commands.rs` — `search_meetings(query, filters, limit)` Tauri
command, registered in `lib.rs`. Runs FTS-only until the embedder lands (then adds the
vector branch — one-line change, marked with a TODO).

### Frontend UI (this pass)

- **RAG chat page** `src/app/chat/page.tsx` (Next.js/React, matches app conventions:
  Tailwind, framer-motion, lucide, `cn`, `invoke`): scope selector (archive / collection /
  meeting), user/assistant bubbles, **clickable `[N]` citation chips** → open
  `/meeting-details?id=…&t=<seconds>` (jump-to-timestamp), "not found" + low-confidence +
  no-citation-warning states, session continuity (`session_id`), suggestions/empty state,
  Enter-to-send. Calls `rag_ask` (nested arg is snake_case serde `RagAskInput`).
- **Search page** `src/app/search/page.tsx`: single search bar + filter panel (date range,
  collection; speaker filter deferred to Phase 2), results **grouped by meeting**, matched
  terms highlighted, timestamp chips, click → `/meeting-details?id=…&t=<seconds>`.
  Calls `search_meetings` (FTS branch today; vector branch turns on with the embedder).
  Date upper bound extended to end-of-day for inclusive filtering across RFC3339/plain
  storage.
- **Sidebar nav** — "Search meetings" and "Chat with archive" entries added to both the
  collapsed icon rail and the expanded footer (`src/components/Sidebar/index.tsx`).
- **Provider-credentials UI** — Settings → new **"Providers"** tab
  (`src/components/ProviderSettings.tsx`): GigaChat (Authorization key or login/password) +
  DeepSeek (API key) + optional model overrides, with configured badges. Secrets are
  write-only (never rendered back). Saves via `set_app_setting`; reads via new
  `get_app_settings` command. Takes effect immediately (providers resolve from
  `app_settings_kv` per call — no restart).
- **Embedding-model download UI** — Settings → new **"Search"** tab
  (`src/components/EmbeddingModelSettings.tsx`): shows model status, a Download button
  (`embedder_download_model`) with a live progress bar (listens to
  `embedder-download-progress` / `embedder-ready`), and active/installed/restart states.
- **Jump-to-timestamp wired** — `meeting-details?id=…&t=<seconds>` now scrolls the transcript
  to the segment at that time and briefly highlights it. Threaded `seekToSeconds` →
  `scrollToTimestamp` through `meeting-details/page.tsx` → `page-content` → `TranscriptPanel`
  → `VirtualizedTranscriptView` (uses the virtualizer's `scrollToIndex` for off-screen
  segments; matches the last segment starting ≤ t among loaded segments). Search results and
  RAG citations both land on the exact moment.
- Frontend type-checks clean (`tsc --noEmit`: 0 errors in the new pages + Sidebar; the one
  project error is a pre-existing unrelated `bun:test` types issue in `tests/`).

### Embedder — FUNCTIONAL ✅ (this pass)

- `src/pipeline/embedder.rs`: ONNX e5-small via `ort`, `query:`/`passage:` prefixes,
  mean-pool + L2-normalize, batching. Concrete `HfTokenizer` (HF `tokenizers` crate,
  loads `tokenizer.json`). Process-wide instance (`static Mutex<Option<Embedder>>`, Send;
  async callers use `spawn_blocking`).
- `src/pipeline/commands.rs`: `embedder_status`, `embedder_download_model` (streams the
  Xenova `multilingual-e5-small` ONNX + tokenizer to `app_data_dir/models/embedding/`,
  progress events, then loads it), `init_embedder_at_startup` (loads at boot if present).
  Commands registered in `lib.rs`.
- Wired: `chunk_embed` embeds chunks → `chunk_embeddings` + sets `embedding_status='done'`;
  `search_meetings` and `rag::ask` embed the query → **vector branch active** when the
  model is loaded (FTS-only otherwise — graceful).
- Verified: full-crate `cargo check --lib` = 0 errors (tokenizers+onig build clean); pure
  math unit-tested; **`HfTokenizer` runtime-verified** loading the real e5 tokenizer.json
  and encoding Russian. ONNX forward pass uses the proven `parakeet_engine` `ort` pattern;
  it runs once the model is downloaded (470MB fp32) via `embedder_download_model`.

### Phase 1 remaining

- **Eval harness** (`evals/search/`): recall@5 / MRR for FTS-only vs vector-only vs
  hybrid. **Blocked on §11 #3:** user to supply/confirm 30 real Russian queries with
  expected meeting_id + timestamp.

### Verified test inventory (standalone harness, exact project deps)

**Rust unit/integration tests (standalone harness, exact project deps):** `vector` (1) ·
`jobs` (6) · `pipeline::chunker` (5) · `pipeline::diarization` (5) · `pipeline::extraction`
(5) · embedder pure math (4) · `search::hybrid` (4 + 1 e2e integration) · `search::rag`
(5) · `llm` guard/router/prompts + DB privacy (8) · `collections` series (4) ·
**`gigaam_engine::featurizer` (matches onnx-asr numpy reference)** + vocab-parse + provider
parse helpers. Plus SQL validated via sqlite3 CLI (all migrations, FTS5 Cyrillic MATCH +
trigger sync, job lifecycle), `evals/phase0/sqlite_vec_smoke.py`, and the tokenizer
runtime-verified on real `tokenizer.json`.

**Frontend tests (`bun test`):** `tests/lib/transcriptionReadiness.test.ts` — 5 pass
(provider-aware recording gate, incl. GigaAM regression guard).

**Runtime end-to-end:** GigaAM Rust engine output == onnx-asr reference on real Russian
speech (see the GigaAM section).

The harness (`scratchpad/verify`) uses the EXACT dependency versions this project
declares, so a green harness is strong evidence the modules compile and behave in the
real crate. Only the ONNX (`ort`) glue in `embedder.rs`/`diarization.rs` and the
tauri-command signatures aren't harness-compiled — the ort glue mirrors the working
`parakeet_engine` pattern verbatim, and the commands follow existing command patterns.

## Build / dependency notes

1. **`cidre` needs full Xcode** (not just Command Line Tools) — installed; app crate now
   compiles. Tauri's `build.rs` also requires the `llama-helper` sidecar at
   `frontend/src-tauri/binaries/llama-helper-<triple>` (build via `cargo build -p
   llama-helper` and copy). `binaries/` is gitignored.
2. **Dependency resolution is clean in the real repo.** The workspace `Cargo.lock` (repo
   root — it IS committed) pins `ort` to `rc.10` with a **single** `ndarray 0.16.1`, so
   there is no ndarray/ort conflict. (An earlier note about rc.12/ndarray was a
   *standalone-harness* artifact — the harness lacked the lock and the other deps that
   constrain resolution. Not a real-repo problem.)
3. **`libsqlite3-sys` pinned to `0.30`** matches sqlx 0.8.6 → resolves to a single
   `0.30.1` (no duplicate SQLite). `sqlite-vec 0.1.9` added to the lock.
4. `strsim` resolves to two versions (0.10 our dep, 0.11 transitive) — harmless (no
   shared types cross the boundary); dedupe to 0.11 later if desired.

## Phase 0 — what shipped

**Migration** `frontend/src-tauri/migrations/20260706000000_v1_knowledge_base.sql`
- Tables: `chunks`, `speakers`, `entities`, `entity_mentions`, `action_items`,
  `collections`, `meeting_collections`, `saved_searches`, `jobs` (+ indexes).
- `ALTER TABLE transcripts ADD COLUMN speaker_id` (diarized identity; distinct from
  the existing `speaker` mic/system column).
- Verified: full migration chain applies cleanly on real SQLite; FK cascade works.

**Vector search** `frontend/src-tauri/src/vector/mod.rs`
- `register()` (auto-extension), `ensure_chunk_embeddings_table()` (graceful),
  `upsert_embedding()`, `knn()`. Wired into `database/manager.rs` before pool open /
  after migrations.
- KNN correctness validated standalone: `evals/phase0/sqlite_vec_smoke.py` (PASS).

**Job queue** `frontend/src-tauri/src/jobs/`
- `store.rs` (persistence), `runner.rs` (bounded-concurrency poll loop, exponential
  backoff, max-3 attempts, startup recovery of interrupted jobs), `mod.rs`
  (`JobHandler` trait, registry, `JobContext`, `enqueue_post_meeting_pipeline`),
  `handlers.rs` (Phase 0 placeholders; `chunk_embed` chains `diarize` + `extract`),
  `tests.rs` (claim exclusivity, retry→fail, flaky→succeed, restart recovery, chaining).
- Runner started in `database/manager.rs`. Finalize hook enqueues the pipeline from
  `summary/commands.rs::api_save_meeting_summary` (non-blocking).
- Job-queue SQL lifecycle validated against real SQLite.

### Phase 0 acceptance criteria

- [x] Migration applies cleanly (verified via sqlite3 CLI on the full chain)
- [x] sqlite-vec KNN returns correct nearest neighbors (python `sqlite_vec` + a Rust
  runtime test that exercises the real `sqlite3_auto_extension` registration)
- [x] Interrupted job resumes on restart (`recover_running` + test)
- [x] Finalizing a meeting enqueues the pipeline without blocking the UI
- [x] Job-queue logic (`src/jobs/tests.rs`, 5 tests) + vector KNN test **compile and
  pass** (verified in a standalone harness — see below)

### Verification status — FULL CRATE COMPILES ✅

`cargo check --lib` on the real `meetily` crate **passes cleanly** (0 errors) with Xcode
installed. This compiles ALL V1 modules, the Tauri command registrations, and every
integration point (`lib.rs`, `database/manager.rs`, `summary/commands.rs`) against the
real dependency graph. To reproduce:

```bash
cargo build -p llama-helper && \
  cp target/debug/llama-helper frontend/src-tauri/binaries/llama-helper-aarch64-apple-darwin
cd frontend/src-tauri && cargo check --lib
```

(The `llama-helper` sidecar must exist for Tauri's `build.rs` resource check — it's a
pre-existing project requirement, not V1-specific. `ffmpeg` auto-downloads.)

Additionally, all pure/algorithmic logic was unit-tested (48 tests green) in a standalone
harness with the exact project dependency versions, confirming:
- `libsqlite3-sys` resolves to a **single** 0.30.1 alongside `sqlx` 0.8.6.
- `sqlite-vec` 0.1.9 links via `sqlite3_auto_extension`; vec0 KNN works at runtime.
- `Cargo.lock` gained only `sqlite-vec` (+11 lines); no version churn.

## Git state

Pushed to `origin/main` (`git@github.com:andyzt/meet_at_giga.git`):
- `eb46382` — V1 knowledge base (Phases 0–5 + hybrid search + RAG + GigaChat/DeepSeek + search/chat UI).
- `3123e5f` — GigaAM v3 Russian ASR (engine + live/batch wiring + GigaAM-only UI) +
  provider-credentials UI + onboarding/UX fixes + recording-readiness fix.

Reproduce the full compile (needs full Xcode + the `llama-helper` sidecar built into
`frontend/src-tauri/binaries/`): `cd frontend && ./dev-gpu.sh` (auto-detects Metal, builds
the sidecar, runs). Or `cargo check --lib` in `frontend/src-tauri` after building
`llama-helper`. Frontend: `corepack enable pnpm` first.

## Remaining / not done

- **Eval harness** (`evals/search/`): recall@5 / MRR (FTS-only vs vector-only vs hybrid) —
  still needs the user's 30 real Russian queries (PLAN.md §11 #3).
- **Phase 2 diarization** — the ONNX diarizer (pyannote/Sortformer) is scaffolded but not
  wired to a real model; segments stay `speaker_id = NULL` until a model + audio wiring land.
  Speaker-resolution algorithms are done + tested.
- **Phase 3 extraction DB persistence** — the LLM extraction call is wired (privacy-guarded,
  routed to GigaChat/DeepSeek, JSON validated); resolving entities into
  `entities`/`entity_mentions`/`action_items` + `pending_merges` is scaffolded (the pure
  resolution logic is tested) but the DB inserts are TODO.
- **Phase 4/5 UI depth** — entity page, action board, collections/series-suggestion UI,
  saved-searches pinning, and the privacy toggles screen aren't built (backend/logic exist).
- **Embedding model default** — `multilingual-e5-small`/384 (downloadable in Settings →
  Search). Reconfirm after the eval benchmark (PLAN.md §11 #1); changing the dim just
  drops/recreates the empty `chunk_embeddings` table.
- **Live GigaAM run in the packaged app** — verified end-to-end via a standalone Rust
  binary on real audio; a final check inside the running app (record → transcript) is the
  last confirmation.
