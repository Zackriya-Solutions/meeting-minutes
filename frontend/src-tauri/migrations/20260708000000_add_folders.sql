-- Organizational folders for grouping meetings (Granola-style sidebar).
-- Distinct from meetings.folder_path, which stores the on-disk recording location.

CREATE TABLE IF NOT EXISTS folders (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL
);

ALTER TABLE meetings ADD COLUMN folder_id TEXT REFERENCES folders(id);

CREATE INDEX IF NOT EXISTS idx_meetings_folder_id ON meetings(folder_id);
