-- Folders, tags, and meeting Trash. New rows use UUID ids, updated_at, and
-- deleted_at tombstones so they can be replicated later.

CREATE TABLE IF NOT EXISTS folders (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    parent_id   TEXT REFERENCES folders(id),
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    deleted_at  TEXT
);
-- COALESCE so top-level folders (parent_id NULL) collide with each other
CREATE UNIQUE INDEX IF NOT EXISTS idx_folders_live_name
    ON folders (COALESCE(parent_id, ''), name COLLATE NOCASE) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS tags (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    deleted_at  TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_tags_live_name
    ON tags (name COLLATE NOCASE) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS meeting_tags (
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    tag_id      TEXT NOT NULL REFERENCES tags(id),
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    deleted_at  TEXT,
    PRIMARY KEY (meeting_id, tag_id)
);
CREATE INDEX IF NOT EXISTS idx_meeting_tags_tag ON meeting_tags (tag_id) WHERE deleted_at IS NULL;

ALTER TABLE meetings ADD COLUMN folder_id TEXT REFERENCES folders(id);
ALTER TABLE meetings ADD COLUMN deleted_at TEXT;
CREATE INDEX IF NOT EXISTS idx_meetings_folder ON meetings (folder_id);
CREATE INDEX IF NOT EXISTS idx_meetings_deleted ON meetings (deleted_at);
