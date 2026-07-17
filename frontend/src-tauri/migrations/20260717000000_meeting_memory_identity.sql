-- First-class Memento memory identity. The selected workflow belongs to the meeting,
-- rather than being a transient summary dropdown choice.

ALTER TABLE meetings ADD COLUMN memory_type TEXT NOT NULL DEFAULT 'general'
    CHECK (memory_type IN ('general', 'standup', 'interview'));

ALTER TABLE meetings ADD COLUMN sensitivity TEXT NOT NULL DEFAULT 'standard'
    CHECK (sensitivity IN ('standard', 'sensitive'));

-- Reviewed Standup V2 records are strong enough evidence to classify existing meetings.
-- Do not infer a type from titles alone.
UPDATE meetings
SET memory_type = 'standup'
WHERE EXISTS (
    SELECT 1 FROM standup_records sr WHERE sr.meeting_id = meetings.id
);

CREATE INDEX IF NOT EXISTS idx_meetings_memory_type
    ON meetings(memory_type, sensitivity, created_at DESC);
