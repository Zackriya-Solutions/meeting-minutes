CREATE TABLE IF NOT EXISTS people (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_people_name ON people (name COLLATE NOCASE);

ALTER TABLE meeting_speakers ADD COLUMN person_id TEXT REFERENCES people(id) ON DELETE SET NULL;
ALTER TABLE meeting_speakers ADD COLUMN name_source TEXT;          -- 'user' | 'voice' | 'conversation'
ALTER TABLE meeting_speakers ADD COLUMN suggested_person_id TEXT;  -- with suggested_name for new people
ALTER TABLE meeting_speakers ADD COLUMN suggested_name TEXT;
ALTER TABLE meeting_speakers ADD COLUMN suggestion_source TEXT;    -- 'voice' | 'conversation'
ALTER TABLE meeting_speakers ADD COLUMN suggestion_reason TEXT;    -- "voice match 0.68", "addressed as Noah at 01:12"

CREATE TABLE IF NOT EXISTS speaker_rejections (
    meeting_id  TEXT NOT NULL,
    speaker_key TEXT NOT NULL,
    person_id   TEXT NOT NULL,
    PRIMARY KEY (meeting_id, speaker_key, person_id),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE,
    FOREIGN KEY (person_id) REFERENCES people(id) ON DELETE CASCADE
);
