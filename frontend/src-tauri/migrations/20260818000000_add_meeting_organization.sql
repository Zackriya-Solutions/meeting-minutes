-- Project folders are intentionally flat. A NULL meeting project_folder_id is Unfiled.
CREATE TABLE IF NOT EXISTS project_folders (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

ALTER TABLE meetings ADD COLUMN project_folder_id TEXT REFERENCES project_folders(id) ON DELETE SET NULL;

CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS meeting_tags (
    meeting_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    PRIMARY KEY (meeting_id, tag_id),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_meetings_project_folder_id ON meetings(project_folder_id);
CREATE INDEX IF NOT EXISTS idx_meeting_tags_tag_id ON meeting_tags(tag_id);
