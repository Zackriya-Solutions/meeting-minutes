# AGENTS.md — Meetily personal fork (single source of truth)

Thai-aware, 100%-local meeting transcription on top of the Tauri/Rust/Next.js
Meetily app. This file is the working memory for the personal-fork features
(file output, Thai STT glossary, translation, summary, local memory search).

## Golden rules
- **Local only.** No outbound network except `127.0.0.1` (Ollama on :11434, ML
  sidecar on :8178). Never add cloud calls.
- **No hardcoded paths.** Everything is read from `.env` / env vars via
  `meeting_log::config`. Defaults match the spec so the packaged app still works.
- Heavy ML (translate / embed / Thai segmentation / vector search) lives in the
  Python **sidecar**; the Rust core stays thin and talks to it over local HTTP.
- Keep this file updated as the design changes.

## Models (local)
| Job | Model | Where |
|---|---|---|
| STT | whisper-rs `large-v3` (+ glossary initial_prompt) | in-app (Rust) |
| Translate →EN | `translategemma:4b` (EN-only model) | Ollama (via sidecar) |
| Translate →TH | `qwen3.5:9b` (`think:false`, Thai-capable) | Ollama (via sidecar) |
| Summary + action items | `qwen3.5:9b` primary → `qwen2.5:14b` fallback | Ollama (via Rust) |
| Embeddings | `bge-m3` (1024-dim) | Ollama (via sidecar) |
| Thai segmentation | PyThaiNLP `newmm` | sidecar |
| Vector store | `sqlite-vec` + FTS5 | sidecar |

All four Ollama models are pulled: `translategemma:4b`, `bge-m3`, `qwen2.5:14b`,
plus the pre-existing `qwen3.5:27b`.

## Configuration — `.env` (repo root, gitignored)
Loaded by `meeting_log::config` from env vars → `.env` (cwd / exe dir / app
support) → baked-in defaults. `.env.example` is the template. Keys:
`MEET_LOG_ROOT`, `MEET_LOG_DATE_FORMAT`, `MEET_TIME_FORMAT`,
`MEET_TRANSCRIPT_PATTERN`, `MEET_SUMMARY_PATTERN`, `MEET_DAILY_LOG`,
`STT_MODEL`, `TRANSLATE_MODEL` (→EN), `TRANSLATE_MODEL_TH` (→TH),
`TRANSLATE_DEFAULT_TARGET`, `SUMMARY_MODEL` (+`SUMMARY_MODEL_FALLBACK`),
`EMBED_MODEL`, `OLLAMA_URL`, `MEET_SIDECAR_URL`, `VECTOR_DB_PATH`, `STT_GLOSSARY`,
`NOTE_LOG_ROOT`, `NOTE_ROLLOVER`.
Java-style date tokens (`yyyy-MM-dd`, `HH-mm-ss`) are auto-translated to chrono.

## Translation (Feature 1)
Language view in the transcript toolbar: Original / ไทย / English / Bilingual,
plus a "🇹🇭 แปลเป็นไทย" quick action (= Bilingual). Per-segment, per-target cache.
Routing: →EN uses `translategemma:4b` (English-only); →TH uses
`TRANSLATE_MODEL_TH` (`qwen3.5:9b`) because translategemma cannot produce Thai.
The sidecar uses a **Thai-language** system prompt for →TH and sends
`think:false` (keeps qwen3 fast, ~1s/segment). Technical terms stay in English
via the glossary. Code in `meeting_log/translate.rs` + sidecar `/translate`
(`target` field) + frontend `lib/meetLog.ts` + `TranscriptView.tsx`.

## Summary model + fallback (Feature 3)
`meeting_log/models.rs` resolves the summary model at call time: runtime override
(Settings dropdown) → env `SUMMARY_MODEL` → env `SUMMARY_MODEL_FALLBACK` → primary
anyway. Checks `ollama /api/tags`; never crashes if a model is missing (logs a
warning). Settings → "Ai Summary" tab has the picker (`MeetLogSummaryModel.tsx`,
commands `meeting_log_list_models` / `_set_summary_model` / `_get_summary_model`).

## Quick Note (Feature 2)
SQLite `quick_notes(id,date,text,done,created_at,carried_from,archived)`
(migration `20260624000000_add_quick_notes.sql`). `meeting_log/notes.rs` has CRUD
commands + `quick_notes_rollover`. **Rollover (NOTE_ROLLOVER=on_launch)**, run from
`layout.tsx` after onboarding: for each un-archived day < today → write
`NOTE_LOG_ROOT/<date>.md` (`[x] … ✅` / `[ ] … ❌`), copy pending cards to today
with `carried_from` set, mark the day archived (idempotent). UI: floating board
`QuickNote.tsx` (⌘⇧N or window event `open-quick-note`) — checkbox, inline edit,
delete, "↩ เมื่อวาน" badge for carried cards.

## File output (spec §3)
Written under `MEET_LOG_ROOT/<date>/`:
- `<time>_transcript.md` — header + one line per **final** segment, flushed to
  disk immediately (crash-safe). `[HH:MM:SS] text` (optional `Speaker:`).
- `<time>_summary.md` — written at session end from the summary model's JSON.
- `summary.md` — per-day log; one appended section per session.

## Architecture / where things live
**Rust** — `frontend/src-tauri/src/meeting_log/`
- `config.rs` — env/.env/defaults loader (+ token translation). Unit-tested.
- `session.rs` — live session state; `start_session`, `append_final_segment`
  (flush), `take_session`. Global `Mutex<Option<SessionLog>>`.
- `summary.rs` — Ollama JSON summary → markdown; writes summary file + daily log;
  spawns memory indexing. Falls back to a minimal summary if the model fails.
- `memory.rs` — chunks a session, calls sidecar `/index` and `/search`.
- `translate.rs` — calls sidecar `/translate`; skips English-only text.
- `sidecar.rs` — best-effort sidecar autostart at app launch (health-check then
  spawn `sidecar/run.sh`).
- `commands.rs` — Tauri commands: `meeting_log_config`, `meeting_log_translate`,
  `meeting_log_search`, `meeting_log_reveal`.

**Lifecycle hooks** (`audio/recording_commands.rs`)
- start → `meeting_log::begin(meeting_name)`
- each `transcript-update` with `!is_partial` → `meeting_log::record_final`
- stop → spawn `meeting_log::end()` → emits `meeting-log-finalized`

**Glossary** (`whisper_engine/whisper_engine.rs`) — glossary joined and set as
`params.set_initial_prompt(...)` at both transcribe call sites (declared before
`params` for lifetime safety).

**Python sidecar** — `sidecar/` (FastAPI, 127.0.0.1:8178)
- `app.py` — `/health /translate /embed /segment /index /search`. Hybrid search =
  dense (bge-m3 + sqlite-vec KNN) + sparse (FTS5 bm25 over newmm-segmented text)
  fused with Reciprocal Rank Fusion. Degrades gracefully if vec/thai missing.
- `requirements.txt`, `run.sh` (venv bootstrap). Verified on Python 3.14.

**Frontend** (`frontend/src`)
- `lib/meetLog.ts` — translation modes A/B/C, per-segment translate cache, copy.
- `components/TranscriptView.tsx` — toolbar (mode toggle A▸B▸C + Copy-all), per-
  line hover copy `⧉`, bilingual EN sub-line, partial=faded/final=solid, toast.
- `components/MeetLogSearch.tsx` — floating memory-search widget (⌘⇧F): snippet +
  date + topics, click reveals the file. Mounted in `app/layout.tsx`.

## Translation modes (spec §5/§7)
A = Original · B = Bilingual (TH + EN sub-line) · C = English. Mode persisted in
localStorage. Sidecar prompt pins technical terms (Kafka/ACL/gRPC/…). Verified:
"เดี๋ยว deploy Kafka ก่อน แล้วเช็ค consumer lag" → "First, let's deploy the Kafka
system and then check the consumer lag." (terms preserved).

## Run / build
**Dev (running from source):**
1. `cd sidecar && ./run.sh` (first run builds the venv) — or let the app autostart it.
2. `cd frontend && pnpm install && pnpm run tauri:dev:metal`

**Build prerequisites:** Rust (rustup, stable), cmake (brew), pnpm pinned to
**9.15.9** (corepack `pnpm@latest` crashes on Node 20), sidecar venv, all Ollama
models, and **full Xcode** (the `cidre` system-audio dep runs `xcodebuild`):
`sudo xcode-select -s /Applications/Xcode.app/Contents/Developer && sudo xcodebuild -license accept`.

**Packaged `.dmg` — DONE.** `./build_meetily.sh` produces it. Two external
binaries must exist under `frontend/src-tauri/binaries/` (the script builds/copies
them): `ffmpeg-<triple>` (auto-downloaded by the build) and
`llama-helper-<triple>` (built from the `llama-helper/` crate with `--features
metal`, copied from `target/release/llama-helper`). Output:
`target/release/bundle/dmg/meetily_0.4.0_aarch64.dmg`.

Notes: the app is **ad-hoc signed** (`signingIdentity: "-"`) — on first launch use
right-click → Open (or `xattr -dr com.apple.quarantine <app>`). The build's final
non-zero exit is only the updater-artifact signing (needs
`TAURI_SIGNING_PRIVATE_KEY`); it runs after the `.dmg` is written, so the `.dmg`
is the source of truth.

## Validation status
- Rust `meeting_log` + `sidecar` modules: `cargo check` clean (isolated crate;
  full app build pending Xcode).
- Frontend: `tsc --noEmit` → 0 errors.
- Sidecar: live-tested `/health /segment /translate /index /search` against real
  models — hybrid search ranks correctly for English and Thai+English queries.

## TODO / later
- Speaker diarization (pyannote) — `record_final` already accepts an optional
  speaker; transcript format supports `Speaker:`.
- README auto-generation per meeting.
- Expose the search layer as an MCP server (search already factored in `memory.rs`
  + sidecar `/search`).
- Bundle the Python sidecar into the packaged app (currently dev-path autostart).
