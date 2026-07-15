-- Reviewable Standup V2 records and their connection to the existing action lifecycle.
-- Generated facts remain pending until the user explicitly accepts them.

-- `created_at` is the database/import timestamp. Recurring workflows need the time at which
-- the meeting actually happened. Safely renamed imports use `YYYY-MM-DD_HH-MM_...`; preserve
-- that source-local time without pretending that its timezone is known.
ALTER TABLE meetings ADD COLUMN occurred_at TEXT;

UPDATE meetings
SET occurred_at = substr(title, 1, 10) || 'T' ||
                  replace(substr(title, 12, 5), '-', ':') || ':00'
WHERE date(substr(title, 1, 10)) IS NOT NULL
  AND substr(title, 11, 1) = '_'
  AND time(replace(substr(title, 12, 5), '-', ':')) IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_meetings_occurred_at ON meetings(occurred_at);

CREATE TRIGGER IF NOT EXISTS set_imported_meeting_occurred_at
AFTER INSERT ON meetings
WHEN NEW.occurred_at IS NULL
 AND date(substr(NEW.title, 1, 10)) IS NOT NULL
 AND substr(NEW.title, 11, 1) = '_'
 AND time(replace(substr(NEW.title, 12, 5), '-', ':')) IS NOT NULL
BEGIN
    UPDATE meetings
    SET occurred_at = substr(NEW.title, 1, 10) || 'T' ||
                      replace(substr(NEW.title, 12, 5), '-', ':') || ':00'
    WHERE id = NEW.id;
END;

CREATE TABLE IF NOT EXISTS standup_records (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    record_key TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN (
        'overview', 'participant_update', 'decision', 'action',
        'risk', 'deep_dive', 'unattributed_fact'
    )),
    payload TEXT NOT NULL,
    reviewed_payload TEXT,
    review_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (review_status IN ('pending', 'accepted', 'rejected')),
    source_schema_version TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(meeting_id, record_key)
);

CREATE INDEX IF NOT EXISTS idx_standup_records_meeting_status
    ON standup_records(meeting_id, review_status, kind);

ALTER TABLE action_items ADD COLUMN standup_record_id INTEGER
    REFERENCES standup_records(id) ON DELETE CASCADE;
ALTER TABLE action_items ADD COLUMN due_date_raw TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_action_items_standup_record
    ON action_items(standup_record_id)
    WHERE standup_record_id IS NOT NULL;
