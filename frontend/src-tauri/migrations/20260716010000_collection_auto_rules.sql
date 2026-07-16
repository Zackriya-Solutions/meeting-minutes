-- Persist honest, user-controlled auto-membership rules for recurring series.
-- Manual collections remain fully manual.
ALTER TABLE collections ADD COLUMN auto_add INTEGER NOT NULL DEFAULT 0;
ALTER TABLE collections ADD COLUMN match_rule TEXT;

CREATE INDEX IF NOT EXISTS idx_collections_auto_add
    ON collections(kind, auto_add);

CREATE TABLE IF NOT EXISTS collection_auto_exclusions (
    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    PRIMARY KEY (collection_id, meeting_id)
);
