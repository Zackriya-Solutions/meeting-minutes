-- Phase 3 (entity merge review), Phase 4 (RAG chat state), Phase 5 (privacy/config kv).

-- Fuzzy-match review queue (PLAN.md Phase 3): 0.85–0.92 similarity band never merges
-- silently; it lands here for user review.
CREATE TABLE IF NOT EXISTS pending_merges (
    id INTEGER PRIMARY KEY,
    entity_id INTEGER REFERENCES entities(id) ON DELETE CASCADE,  -- existing candidate
    incoming_name TEXT NOT NULL,
    incoming_type TEXT NOT NULL,
    incoming_aliases TEXT NOT NULL DEFAULT '[]',
    score REAL NOT NULL,
    meeting_id TEXT REFERENCES meetings(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending','merged','rejected')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_pending_merges_status ON pending_merges(status);

-- RAG chat state (PLAN.md Phase 4): one session per scope, messages with citations.
CREATE TABLE IF NOT EXISTS chat_sessions (
    id INTEGER PRIMARY KEY,
    scope TEXT NOT NULL,          -- 'archive' | 'collection:<id>' | 'meeting:<uuid>'
    title TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS chat_messages (
    id INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('user','assistant')),
    content TEXT NOT NULL,
    citations TEXT NOT NULL DEFAULT '[]',  -- JSON: [{chunk_id, meeting_id, start_ms}]
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_chat_messages_session ON chat_messages(session_id);

-- Generic key/value app settings (PLAN.md Phase 5 privacy surface + tunable thresholds).
-- Privacy keys: 'privacy.local_only', 'privacy.extraction_enabled', 'privacy.chat_enabled'.
-- Threshold keys: 'speaker.tau', 'action.dedupe_threshold'.
CREATE TABLE IF NOT EXISTS app_settings_kv (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
