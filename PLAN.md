# V1 Implementation Plan — Meeting Logs Management

> **Purpose of this document:** Executable implementation spec for Claude Code.
> Work through phases in order. Each phase has explicit tasks, file targets, and acceptance criteria.
> Do not start a phase until the previous phase’s acceptance criteria pass.

-----

## 1. Project Context

**Product:** Privacy-first desktop meeting assistant (Meetily fork) for the Russian market.
Records meetings (mic + system audio), transcribes locally with **GigaAM v3 e2e** (ONNX), summarizes via **GigaChat / DeepSeek** router.

**V1 goal:** Turn the meeting archive into a searchable, queryable knowledge base:
semantic + hybrid search, speaker profiles, cross-meeting entities, action-item tracking, RAG chat over the archive.

**Assumed MVP baseline (already implemented — verify before starting):**

- [ ] Tauri app: Rust backend, Next.js frontend
- [ ] GigaAM v3 e2e ONNX transcription pipeline with Silero VAD chunking (≤25s segments)
- [ ] SQLite storage with segment-level transcripts: `segments(meeting_id, start_ms, end_ms, text)`
- [ ] FTS5 full-text search over transcripts
- [ ] Meeting entity: title, datetime, duration, tags, audio file path
- [ ] LLM provider layer with OpenAI-compatible endpoint support (DeepSeek direct, GigaChat via adapter)

If any baseline item is missing, STOP and report — do not silently implement it as part of V1.

**Tech constraints:**

- All inference local via `onnxruntime` (already shipped for GigaAM). No Python runtime in production builds.
- SQLite is the only database. Vector search via `sqlite-vec` extension.
- Transcript text may be sent to LLM APIs (GigaChat/DeepSeek) ONLY for summarization, extraction, and chat. Embeddings and search must be fully local.
- Primary language of content: Russian. All prompts, chunking, and embedding choices must be validated on Russian text.

-----

## 2. Architecture Overview

```
Recording → GigaAM ASR → segments (SQLite)
                              │
                    [post-meeting pipeline — background job queue]
                              │
        ┌─────────────┬───────┴────────┬──────────────────┐
        ▼             ▼                ▼                  ▼
   Diarization    Chunking +      LLM extraction     Summary
   (pyannote      Embedding       (entities,         (existing MVP)
    ONNX)         (local ONNX)    action items)
        │             │                │
        ▼             ▼                ▼
    speakers      chunks +        entities,
    table         vec index       action_items
                      │
                      ▼
            Hybrid search (FTS5 + sqlite-vec + RRF)
                      │
                      ▼
                 RAG chat with citations
```

**Job queue:** all post-meeting processing runs as background jobs (Rust, `tokio` tasks + a `jobs` table for persistence/retry). Meeting finalization must never block the UI.

-----

## 3. Phase 0 — Data Foundation

**Goal:** schema, migrations, job queue. Everything else depends on this.

### Tasks

1. **Migrations tooling.** If not present, add `sqlx` migrations (or `rusqlite_migration`). All schema changes below go through numbered migration files in `src-tauri/migrations/`.
1. **Schema.** Create migration with:

```sql
-- Retrieval units for semantic search (~200–400 tokens, 1–2 segment overlap)
CREATE TABLE chunks (
    id INTEGER PRIMARY KEY,
    meeting_id INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    first_segment_id INTEGER NOT NULL,
    last_segment_id INTEGER NOT NULL,
    start_ms INTEGER NOT NULL,
    end_ms INTEGER NOT NULL,
    text TEXT NOT NULL,
    token_count INTEGER NOT NULL,
    embedding_status TEXT NOT NULL DEFAULT 'pending'  -- pending | done | failed
);

-- sqlite-vec virtual table; dimension set by chosen embedder (see Phase 1)
CREATE VIRTUAL TABLE chunk_embeddings USING vec0(
    chunk_id INTEGER PRIMARY KEY,
    embedding FLOAT[768]  -- adjust to model dim
);

CREATE TABLE speakers (
    id INTEGER PRIMARY KEY,
    display_name TEXT NOT NULL,
    voice_embedding BLOB,            -- averaged, nullable until computed
    is_confirmed INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Add to existing segments table:
ALTER TABLE segments ADD COLUMN speaker_id INTEGER REFERENCES speakers(id);

CREATE TABLE entities (
    id INTEGER PRIMARY KEY,
    type TEXT NOT NULL CHECK (type IN ('project','person','client','topic')),
    canonical_name TEXT NOT NULL,
    aliases TEXT NOT NULL DEFAULT '[]',   -- JSON array
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(type, canonical_name)
);

CREATE TABLE entity_mentions (
    id INTEGER PRIMARY KEY,
    entity_id INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    meeting_id INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    chunk_id INTEGER REFERENCES chunks(id) ON DELETE SET NULL,
    quote TEXT
);

CREATE TABLE action_items (
    id INTEGER PRIMARY KEY,
    meeting_id INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    text TEXT NOT NULL,
    owner_speaker_id INTEGER REFERENCES speakers(id),
    owner_name_raw TEXT,             -- as extracted, before speaker resolution
    due_date TEXT,                   -- ISO date, nullable
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open','done','cancelled','superseded')),
    superseded_by INTEGER REFERENCES action_items(id),
    source_quote TEXT,
    source_start_ms INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE collections (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL DEFAULT 'manual' CHECK (kind IN ('manual','series')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE meeting_collections (
    meeting_id INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    PRIMARY KEY (meeting_id, collection_id)
);

CREATE TABLE saved_searches (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    query TEXT NOT NULL,
    filters TEXT NOT NULL DEFAULT '{}',  -- JSON: date range, speaker_ids, collection_ids
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE jobs (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,              -- chunk_embed | diarize | extract | backfill
    meeting_id INTEGER,
    payload TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ('queued','running','done','failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_jobs_status ON jobs(status, kind);
```

1. **Bundle `sqlite-vec`.** Add the extension to the build for macOS/Windows/Linux; load it at connection open. Smoke-test insert + KNN query in a unit test.
1. **Job runner.** Rust module `src-tauri/src/jobs/`:
- Poll `jobs` table (or notify channel), max N concurrent, exponential backoff, max 3 attempts.
- Job kinds registered via a trait: `trait Job { fn kind() -> &'static str; async fn run(ctx, payload) -> Result<()> }`.
- On meeting finalize: enqueue `diarize` → `chunk_embed` → `extract` (chain: each enqueues the next on success; `chunk_embed` must not wait on `diarize` failure — see Phase 2 degradation rules).

### Acceptance criteria

- [ ] `cargo test` passes with migration up/down tests
- [ ] sqlite-vec KNN query returns correct nearest neighbors on synthetic vectors on all target OS builds
- [ ] Killing the app mid-job → job resumes/retries on restart
- [ ] Finalizing a meeting enqueues the pipeline and UI remains responsive

-----

## 4. Phase 1 — Semantic + Hybrid Search

### Tasks

1. **Embedding model selection (timebox: 2 days).**
   Candidates (multilingual, strong Russian, ONNX-exportable, ≤400M params):
- `intfloat/multilingual-e5-small` / `-base`
- `sergeyzh/rubert-tiny-turbo` or other ru-tuned sentence encoders
- `ai-forever` sentence encoders if ONNX export is clean
  Export to ONNX, run in existing onnxruntime. Benchmark on 30 real Russian meeting queries (see eval task below). Pick by recall@5, break ties by latency/size. **Record the decision and dim in this file (update the `FLOAT[768]` migration accordingly before it ships).**
  Note for e5 family: prefix `query:` / `passage:` conventions are required — encode them in the embedder wrapper, not at call sites.
1. **Chunker.** `src-tauri/src/pipeline/chunker.rs`:
- Input: ordered segments of a meeting. Output: chunks of 200–400 tokens (use the embedder’s tokenizer for counting), overlapping by 1–2 segments.
- Never split inside a segment. Store segment range + ms boundaries.
- Deterministic: same input → same chunks (needed for backfill idempotency).
1. **Embedder.** `src-tauri/src/pipeline/embedder.rs`:
- Batch inference (batch 8–16), L2-normalize, write to `chunk_embeddings`, set `embedding_status='done'`.
- Runs as `chunk_embed` job.
1. **Hybrid query engine.** `src-tauri/src/search/hybrid.rs`:
- Branch A: FTS5 BM25, top 20. Branch B: vector KNN, top 20.
- Fuse with Reciprocal Rank Fusion: `score = Σ 1/(60 + rank_i)`.
- Filters (date range, speaker_ids, collection_ids) applied as SQL predicates on both branches **before** ranking.
- Return: chunk text, meeting metadata, start_ms (for jump-to-timestamp), matched terms for highlighting.
1. **Tauri command + UI.**
- `search(query, filters) -> Vec<SearchResult>` command.
- Single search bar; results grouped by meeting; click → open transcript at timestamp.
- Filter panel: date range, speaker (Phase 2 populates), collection.
1. **Eval harness.** `evals/search/`:
- `queries.jsonl`: 30 queries with expected meeting_id + approximate timestamp (build from real transcripts; ask the user to supply/confirm the query set).
- Script computes recall@5 and MRR for FTS-only, vector-only, hybrid. Run in CI on a fixture DB.

### Acceptance criteria

- [ ] Hybrid recall@5 ≥ vector-only and ≥ FTS-only on the eval set
- [ ] End-to-end: finalize meeting → searchable within 2 min on CPU (M-series / modern x86)
- [ ] Search latency < 300 ms on a 200-meeting archive
- [ ] Russian morphology sanity: query «бюджет» matches «бюджета/бюджетом» via vector branch even when FTS misses

-----

## 5. Phase 2 — Diarization + Speaker Profiles

**Risk note:** highest-risk phase. Hard timebox on model evaluation: 3 days. If quality on Russian multi-speaker audio with crosstalk is unacceptable, ship manual speaker labeling only and file a follow-up — do not stall the phase.

### Tasks

1. **Model evaluation (3-day timebox).**
- Candidates: pyannote `speaker-diarization-3.1` (ONNX-exported), NeMo Sortformer (ONNX).
- Test on 5 real recordings (2–5 speakers, Russian, some crosstalk). Metric: DER + subjective “would a user trust the labels”.
- Deliverable: short report in `docs/diarization-eval.md` + go/no-go decision.
1. **Diarization job** (`diarize`), post-meeting only (NOT live):
- Run on the meeting audio file → speaker turns `(start_ms, end_ms, cluster_id)`.
- Merge with segments: assign each segment the cluster with max time-overlap; ambiguous (<60% overlap) → NULL speaker.
- Compute per-cluster voice embedding (the diarization model’s speaker embedding, averaged).
1. **Speaker identity resolution:**
- Cosine-match cluster embedding vs `speakers.voice_embedding`. Threshold τ (start at 0.75, make configurable): above → auto-assign existing speaker; below → create `Speaker N` (unconfirmed).
- On user rename: set `display_name`, `is_confirmed=1`, fold cluster embedding into the profile (running average).
- Rename propagates via FK — verify all UI reads join through `speakers`.
1. **Degradation rules (must hold everywhere):**
- Diarization job failure must NOT block `chunk_embed`/`extract` (reorder chain: `chunk_embed` first, `diarize` and `extract` in parallel after it).
- All search/RAG/UI paths work with `speaker_id IS NULL`. Speaker filter simply excludes unlabeled segments with a visible note.
1. **UI:** speaker chips on transcript, one-tap rename with autocomplete from existing speakers, speaker filter in search.

### Acceptance criteria

- [ ] On eval recordings: ≥80% of segments correctly attributed after one rename pass per speaker
- [ ] Same real person auto-recognized across two different meetings after confirmation in the first
- [ ] Pipeline completes normally when diarization model is missing/fails
- [ ] Rename in one meeting updates that speaker’s name everywhere instantly

-----

## 6. Phase 3 — Entities + Action Items

### Tasks

1. **Extraction job** (`extract`), post-meeting LLM pass:
- Provider: DeepSeek via existing router (structured extraction task).
- Input: full transcript with speaker labels (if available) + meeting metadata.
- Output: strict JSON (validate against a schema; retry once on invalid JSON):

```json
{
  "entities": [
    {"type": "project|person|client|topic", "name": "...", "aliases": ["..."], "quote": "..."}
  ],
  "action_items": [
    {"text": "...", "owner": "имя или null", "due_date": "YYYY-MM-DD или null", "quote": "...", "approx_position": "начало|середина|конец"}
  ]
}
```

- Prompt in Russian, few-shot with 2 examples. Store prompt in `src-tauri/prompts/extract_v1.md` — prompts are versioned files, not string literals.
- Map `quote` back to a chunk via substring/fuzzy match → fill `entity_mentions.chunk_id`, `action_items.source_start_ms`.

1. **Entity resolution:**
- Normalize (lowercase, trim, ё→е) → exact match on canonical_name or aliases → else fuzzy (Jaro-Winkler ≥ 0.92) → else create new.
- Fuzzy matches in the 0.85–0.92 band → write to a `pending_merges` review queue (simple table + UI list), never silently merge.
1. **Action-item lifecycle:**
- On extraction, dedupe against OPEN items in the same collection/series: embedding cosine ≥ 0.85 → LLM confirm (“same task? yes/no”) → if same, link `superseded_by` and carry status.
- Status changes: manual (UI) + LLM hint when a new meeting mentions completion (surface as suggestion, never auto-close).
1. **UI:**
- Entity page: name, type, aliases, timeline of mentions (meeting + quote + jump link).
- Action board: filterable by status/owner/collection; “open since {date}” badge computed from created_at of the earliest item in the supersede chain.

### Acceptance criteria

- [ ] Extraction JSON validity ≥ 95% over 20 test meetings (with the one retry)
- [ ] «Проект Альфа» and «Альфа» resolve to one entity; «Иванов» (person) and «Иванов и партнёры» (client) do NOT
- [ ] An action item repeated across two meetings appears once on the board with correct “open since” date
- [ ] Zero LLM calls when the user has extraction disabled in settings (privacy toggle, see §8)

-----

## 7. Phase 4 — Chat with Archive (RAG)

### Tasks

1. **Retrieval for RAG:** reuse hybrid engine; top 12 chunks within the selected scope (all / collection / meeting). Optional rerank: score = RRF + recency boost for “latest status” style queries (detect via simple heuristics first; don’t over-engineer).
1. **Answer generation:**
- Prompt (Russian, versioned file `prompts/rag_answer_v1.md`): chunks with `[N]` markers + meeting titles/dates; instruction to answer ONLY from provided context and cite `[N]` per claim.
- Router: single-meeting or lookup questions → GigaChat; cross-meeting synthesis → DeepSeek. Route by scope + query length heuristic; log routing decisions for later tuning.
- **Low-confidence guard:** if top RRF score < threshold or LLM returns the sentinel «в записях не найдено», show “Not found in your meetings” — never a fabricated answer.
1. **Citations UI:** each `[N]` renders as a chip → click opens transcript at chunk start_ms. Answers without at least one citation are rejected and regenerated once, then shown with a warning.
1. **Chat state:** conversation history per scope, stored locally; context window management = last 6 turns + fresh retrieval per turn.

### Acceptance criteria

- [ ] “Что мы решили по X за последний месяц” produces a cited answer spanning ≥2 meetings on test data
- [ ] Question about content absent from archive → explicit “not found”, 0 fabricated claims in a 20-question adversarial test
- [ ] Every claim in answers is clickable to source timestamp
- [ ] Scope selector correctly restricts retrieval (verify with fixture data)

-----

## 8. Phase 5 — Collections, Saved Searches, Backfill, Privacy Surface

### Tasks

1. **Collections:** manual CRUD + auto-series suggestion: normalized-title similarity + recurring cadence detection → propose “Group these 4 meetings into a series?” (one-tap accept).
1. **Saved searches:** persist query+filters, pin to home, run on open.
1. **Backfill command:** `backfill` job iterating all meetings lacking chunks/entities; idempotent (deterministic chunker makes re-runs safe); progress UI; rate-limit LLM extraction calls.
1. **Privacy surface (settings screen):**
- Explicit map: *Local only:* transcription, embeddings, search, diarization. *Sent to API:* summaries, extraction, chat (transcript text to GigaChat/DeepSeek).
- Toggles: disable extraction, disable chat, “local-only mode” master switch. Enforced at the provider layer (guard in the LLM trait), not just UI.

### Acceptance criteria

- [ ] Backfill of a 100-meeting archive completes without manual intervention; interrupt + resume works
- [ ] Local-only mode: network inspector shows zero outbound LLM calls during full pipeline run
- [ ] Series suggestion fires on an obvious weekly-standup fixture set

-----

## 9. Cross-cutting Conventions

- **LLM access:** single trait `LlmProvider` with `complete(request) -> Result<Response>`; router selects provider; every call site passes a `purpose` enum (`Summary | Extract | Chat`) so privacy toggles and logging are enforced centrally.
- **Prompts:** versioned files in `src-tauri/prompts/`, loaded at runtime, name includes version (`extract_v1.md`). Changing a prompt = new file.
- **Errors:** background jobs never surface raw errors to UI; failed jobs show a retriable “processing incomplete” badge on the meeting.
- **Tests:** each phase adds fixture-DB integration tests; eval harnesses (`evals/`) run on demand, not in every CI pass.
- **Do not** introduce a Python runtime, a second database, or a network dependency for any search/embedding path.

## 10. Sequencing & Effort

|Phase|Scope                         |Est.  |Depends on    |
|-----|------------------------------|------|--------------|
|0    |Schema, sqlite-vec, job queue |1 wk  |—             |
|1    |Hybrid search                 |2 wk  |0             |
|2    |Diarization + speakers        |2–3 wk|0             |
|3    |Entities + actions            |2 wk  |0, 1          |
|4    |RAG chat                      |2 wk  |1 (3 enriches)|
|5    |Collections, backfill, privacy|1 wk  |all           |

Phases 1 and 2 are independent after Phase 0 and may be parallelized. Phase 2 has a hard 3-day eval timebox with a defined fallback (manual labeling).

## 11. Open Decisions (resolve with the user before the relevant phase)

1. Embedding model + dimension (Phase 1, after benchmark) — update migration before first release.
1. Diarization go/no-go (Phase 2, day 3).
1. Eval query set: user to provide/confirm 30 real Russian queries with expected answers.
1. Cosine thresholds (speaker τ=0.75, action-dedupe 0.85) — tune on real data, keep configurable.