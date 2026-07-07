-- V1 Knowledge Base foundation (PLAN.md Phase 0 — Data Foundation).
--
-- ADAPTATION NOTE: PLAN.md assumes a GigaAM fork whose `meetings.id` is INTEGER
-- and where segment-level rows live in a `segments` table. In THIS repo:
--   * `meetings.id` and `transcripts.id` are TEXT (UUID strings).
--   * The segment-level table is `transcripts` (columns: transcript, audio_start_time,
--     audio_end_time in SECONDS, speaker), not `segments` with INTEGER ms.
-- Every FK below therefore uses TEXT for meeting/segment references. New tables use
-- INTEGER primary keys (rowids) so they can back the sqlite-vec vec0 table cleanly.
-- Timing in new tables is stored as INTEGER milliseconds (convert from the seconds
-- REAL columns on `transcripts` at write time: ms = round(seconds * 1000)).

-- ---------------------------------------------------------------------------
-- Retrieval units for semantic search (~200-400 tokens, 1-2 segment overlap).
-- first/last_segment_id reference transcripts(id) (TEXT). start_ms/end_ms are the
-- chunk's absolute recording-relative bounds in milliseconds.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS chunks (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    first_segment_id TEXT NOT NULL,
    last_segment_id TEXT NOT NULL,
    start_ms INTEGER NOT NULL,
    end_ms INTEGER NOT NULL,
    text TEXT NOT NULL,
    token_count INTEGER NOT NULL,
    embedding_status TEXT NOT NULL DEFAULT 'pending'  -- pending | done | failed
        CHECK (embedding_status IN ('pending','done','failed')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_chunks_meeting ON chunks(meeting_id);
CREATE INDEX IF NOT EXISTS idx_chunks_embedding_status ON chunks(embedding_status);

-- NOTE: `chunk_embeddings` (sqlite-vec vec0 virtual table) is intentionally NOT
-- created here. vec0 requires the sqlite-vec extension to be loaded on the
-- connection, which is not guaranteed during migration. It is created in code
-- (crate::jobs / crate::vector) after migrations with graceful degradation, so a
-- build without the extension still boots. See src/vector/mod.rs.

-- ---------------------------------------------------------------------------
-- Speaker profiles (diarization + identity resolution, Phase 2).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS speakers (
    id INTEGER PRIMARY KEY,
    display_name TEXT NOT NULL,
    voice_embedding BLOB,             -- averaged voice embedding, nullable until computed
    is_confirmed INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Diarized speaker identity for a transcript segment. This is DISTINCT from the
-- existing `transcripts.speaker` TEXT column (which stores 'mic' | 'system' audio
-- source). speaker_id links to a resolved speaker profile; NULL when unattributed.
ALTER TABLE transcripts ADD COLUMN speaker_id INTEGER REFERENCES speakers(id);

-- ---------------------------------------------------------------------------
-- Cross-meeting entities + mentions (Phase 3).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS entities (
    id INTEGER PRIMARY KEY,
    type TEXT NOT NULL CHECK (type IN ('project','person','client','topic')),
    canonical_name TEXT NOT NULL,
    aliases TEXT NOT NULL DEFAULT '[]',   -- JSON array
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(type, canonical_name)
);

CREATE TABLE IF NOT EXISTS entity_mentions (
    id INTEGER PRIMARY KEY,
    entity_id INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    chunk_id INTEGER REFERENCES chunks(id) ON DELETE SET NULL,
    quote TEXT
);
CREATE INDEX IF NOT EXISTS idx_entity_mentions_entity ON entity_mentions(entity_id);
CREATE INDEX IF NOT EXISTS idx_entity_mentions_meeting ON entity_mentions(meeting_id);

-- ---------------------------------------------------------------------------
-- Action items with lifecycle (Phase 3).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS action_items (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    text TEXT NOT NULL,
    owner_speaker_id INTEGER REFERENCES speakers(id),
    owner_name_raw TEXT,              -- as extracted, before speaker resolution
    due_date TEXT,                    -- ISO date, nullable
    status TEXT NOT NULL DEFAULT 'open'
        CHECK (status IN ('open','done','cancelled','superseded')),
    superseded_by INTEGER REFERENCES action_items(id),
    source_quote TEXT,
    source_start_ms INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_action_items_meeting ON action_items(meeting_id);
CREATE INDEX IF NOT EXISTS idx_action_items_status ON action_items(status);

-- ---------------------------------------------------------------------------
-- Collections / series (Phase 5).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS collections (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL DEFAULT 'manual' CHECK (kind IN ('manual','series')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS meeting_collections (
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    PRIMARY KEY (meeting_id, collection_id)
);

-- ---------------------------------------------------------------------------
-- Saved searches (Phase 5).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS saved_searches (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    query TEXT NOT NULL,
    filters TEXT NOT NULL DEFAULT '{}',  -- JSON: date range, speaker_ids, collection_ids
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ---------------------------------------------------------------------------
-- Background job queue (persistence + retry). Runner lives in src/jobs/.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS jobs (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,               -- chunk_embed | diarize | extract | backfill
    meeting_id TEXT,
    payload TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued','running','done','failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    run_after TEXT,                   -- ISO datetime; NULL = eligible now. Used for backoff.
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status, kind);
