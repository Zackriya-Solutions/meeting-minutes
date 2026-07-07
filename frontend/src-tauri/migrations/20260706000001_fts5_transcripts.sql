-- FTS5 full-text index over transcript segments (PLAN.md Phase 1, branch A / BM25).
--
-- BASELINE GAP: PLAN.md §1 lists FTS5 as an assumed baseline, but upstream Meetily
-- has none. It is added here (rather than silently as "V1") because the Phase 1 hybrid
-- search engine hard-depends on it. See PLAN_PROGRESS.md.
--
-- External-content FTS5: the index reads text from `transcripts` (no duplicated copy)
-- keyed by the table's implicit integer rowid. `transcripts.id` is TEXT, so it is NOT
-- the rowid; the implicit rowid is used for content_rowid (the default). Retrieval
-- joins `transcripts` on rowid to recover id/meeting_id/timing.
CREATE VIRTUAL TABLE IF NOT EXISTS transcripts_fts USING fts5(
    transcript,
    content='transcripts',
    content_rowid='rowid',
    tokenize='unicode61 remove_diacritics 2'
);

-- Backfill any existing rows.
INSERT INTO transcripts_fts(rowid, transcript)
    SELECT rowid, transcript FROM transcripts;

-- Keep the index in sync with the content table.
CREATE TRIGGER IF NOT EXISTS transcripts_fts_ai AFTER INSERT ON transcripts BEGIN
    INSERT INTO transcripts_fts(rowid, transcript) VALUES (new.rowid, new.transcript);
END;

CREATE TRIGGER IF NOT EXISTS transcripts_fts_ad AFTER DELETE ON transcripts BEGIN
    INSERT INTO transcripts_fts(transcripts_fts, rowid, transcript)
        VALUES ('delete', old.rowid, old.transcript);
END;

CREATE TRIGGER IF NOT EXISTS transcripts_fts_au AFTER UPDATE ON transcripts BEGIN
    INSERT INTO transcripts_fts(transcripts_fts, rowid, transcript)
        VALUES ('delete', old.rowid, old.transcript);
    INSERT INTO transcripts_fts(rowid, transcript) VALUES (new.rowid, new.transcript);
END;

-- Chunk-level FTS index (branch A of hybrid search). Both hybrid branches operate at
-- chunk granularity so RRF fuses comparable units: BM25 over `chunks.text` here, and
-- vector KNN over `chunk_embeddings` (keyed by the same chunk id). `chunks.id` is an
-- INTEGER PRIMARY KEY, so it IS the rowid — content_rowid='id'. Empty at creation
-- (chunks are produced by the Phase 1 embedder job), so no backfill is needed.
CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    text,
    content='chunks',
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);

CREATE TRIGGER IF NOT EXISTS chunks_fts_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, text) VALUES (new.id, new.text);
END;

CREATE TRIGGER IF NOT EXISTS chunks_fts_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;

CREATE TRIGGER IF NOT EXISTS chunks_fts_au AFTER UPDATE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.id, old.text);
    INSERT INTO chunks_fts(rowid, text) VALUES (new.id, new.text);
END;
