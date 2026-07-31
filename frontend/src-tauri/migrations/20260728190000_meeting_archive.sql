-- Soft-delete meetings so accidental removal can be reversed from the UI.
ALTER TABLE meetings ADD COLUMN archived_at TEXT;

CREATE INDEX IF NOT EXISTS idx_meetings_archived_at
    ON meetings(archived_at, created_at DESC);
