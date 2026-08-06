-- People a meeting is known to involve, independent of diarization.
--
-- Seeded from the calendar invitation when a recording starts from an Outlook entry,
-- which is knowledge no amount of listening can recover: an invitation names people
-- who never say their own name out loud. Speaker naming reads it to put those names
-- to the separated voices.
--
-- The list is per meeting and never a global address book, so it disappears with the
-- meeting. `normalized_name` exists only to keep one person from being stored twice
-- under different spacing or case.
CREATE TABLE IF NOT EXISTS meeting_participants (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    display_name TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'outlook_calendar',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(meeting_id, normalized_name)
);

CREATE INDEX IF NOT EXISTS idx_meeting_participants_meeting
    ON meeting_participants(meeting_id, id);
