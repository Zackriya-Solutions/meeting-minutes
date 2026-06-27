-- Speaker diarization foundation.
-- Existing transcripts.speaker is legacy source metadata (mic/system) and remains untouched.

ALTER TABLE transcripts ADD COLUMN speaker_id TEXT;
ALTER TABLE transcripts ADD COLUMN speaker_label TEXT;
ALTER TABLE transcripts ADD COLUMN speaker_color TEXT;
ALTER TABLE transcripts ADD COLUMN is_overlap INTEGER NOT NULL DEFAULT 0;
ALTER TABLE transcripts ADD COLUMN diarization_status TEXT NOT NULL DEFAULT 'none';
ALTER TABLE transcripts ADD COLUMN diarization_method TEXT;
ALTER TABLE transcripts ADD COLUMN diarization_confidence REAL;

CREATE TABLE IF NOT EXISTS diarization_settings (
    id TEXT PRIMARY KEY NOT NULL DEFAULT '1',
    enabled INTEGER NOT NULL DEFAULT 0,
    mode TEXT NOT NULL DEFAULT 'live_plus_post_call',
    show_provisional_labels INTEGER NOT NULL DEFAULT 1,
    post_call_refinement_enabled INTEGER NOT NULL DEFAULT 1,
    overlap_handling TEXT NOT NULL DEFAULT 'multiple_speakers',
    speaker_review_enabled INTEGER NOT NULL DEFAULT 1,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO diarization_settings (id)
VALUES ('1')
ON CONFLICT(id) DO NOTHING;

CREATE TABLE IF NOT EXISTS meeting_diarization_status (
    meeting_id TEXT PRIMARY KEY NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0,
    live_enabled INTEGER NOT NULL DEFAULT 0,
    post_call_enabled INTEGER NOT NULL DEFAULT 0,
    current_status TEXT NOT NULL DEFAULT 'none',
    quality_flags TEXT,
    processed_at DATETIME,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS speaker_segments (
    id TEXT PRIMARY KEY NOT NULL,
    meeting_id TEXT NOT NULL,
    source TEXT NOT NULL,
    start_time REAL NOT NULL,
    end_time REAL NOT NULL,
    speaker_id TEXT,
    speaker_label TEXT,
    confidence REAL,
    is_overlap INTEGER NOT NULL DEFAULT 0,
    diarization_status TEXT NOT NULL,
    diarization_method TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_speaker_segments_meeting_time
ON speaker_segments(meeting_id, start_time, end_time);

CREATE INDEX IF NOT EXISTS idx_transcripts_meeting_diarization
ON transcripts(meeting_id, diarization_status);
