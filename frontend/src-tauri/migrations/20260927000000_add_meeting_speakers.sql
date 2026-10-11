-- Per-meeting speakers produced by speaker identification.
-- transcripts.speaker holds speaker_key; display_name NULL means the default "Speaker N" label.
CREATE TABLE IF NOT EXISTS meeting_speakers (
    meeting_id     TEXT NOT NULL,
    speaker_key    TEXT NOT NULL,
    display_name   TEXT,
    embedding      BLOB,
    speech_seconds REAL NOT NULL DEFAULT 0,
    created_at     TEXT NOT NULL,
    PRIMARY KEY (meeting_id, speaker_key),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
