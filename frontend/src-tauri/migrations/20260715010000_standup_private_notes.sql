-- User-authored standup preparation and scratchpad notes.
-- These rows are deliberately separate from transcripts and generated standup_records: they
-- are private local context and must never be presented as speech evidence.

CREATE TABLE IF NOT EXISTS standup_private_notes (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('planned_update', 'parking_lot', 'private_note')),
    text TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'done', 'archived')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_standup_private_notes_meeting_status
    ON standup_private_notes(meeting_id, status, kind, id);
