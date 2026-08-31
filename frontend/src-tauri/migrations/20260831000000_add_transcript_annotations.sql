CREATE TABLE transcript_annotations (
    id TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL,
    annotation_type TEXT NOT NULL,
    anchor_time REAL NOT NULL,
    created_at TEXT NOT NULL,
    text TEXT,
    image_file TEXT,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

CREATE INDEX idx_transcript_annotations_meeting_time
    ON transcript_annotations (meeting_id, anchor_time, created_at);
