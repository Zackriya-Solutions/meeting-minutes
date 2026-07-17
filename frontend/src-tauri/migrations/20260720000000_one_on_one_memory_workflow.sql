-- One-on-One Memory V1. Keep the legacy memory_type CHECK intact and persist the
-- concrete workflow separately; rebuilding meetings would be risky for existing installs.

ALTER TABLE meetings ADD COLUMN summary_template_id TEXT NOT NULL DEFAULT 'standard_meeting';
ALTER TABLE meetings ADD COLUMN occurred_at_confirmed INTEGER NOT NULL DEFAULT 0
    CHECK (occurred_at_confirmed IN (0, 1));

UPDATE meetings SET summary_template_id = CASE memory_type
    WHEN 'standup' THEN 'daily_standup'
    WHEN 'interview' THEN 'interview_memory'
    ELSE 'standard_meeting'
END;

CREATE INDEX IF NOT EXISTS idx_meetings_summary_template
    ON meetings(summary_template_id, sensitivity, created_at DESC);

CREATE TABLE IF NOT EXISTS one_on_one_participant_pairs (
    id INTEGER PRIMARY KEY,
    pair_key TEXT NOT NULL UNIQUE,
    participant_a TEXT NOT NULL,
    participant_b TEXT NOT NULL,
    participant_a_role TEXT,
    participant_b_role TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS one_on_one_configs (
    meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,
    pair_id INTEGER REFERENCES one_on_one_participant_pairs(id) ON DELETE SET NULL,
    participant_a TEXT,
    participant_a_role TEXT,
    participant_b TEXT,
    participant_b_role TEXT,
    shared_agenda_json TEXT NOT NULL DEFAULT '[]',
    target_minutes INTEGER NOT NULL DEFAULT 30 CHECK (target_minutes BETWEEN 10 AND 180),
    facilitation_enabled INTEGER NOT NULL DEFAULT 0 CHECK (facilitation_enabled IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS one_on_one_series (
    meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,
    pair_id INTEGER NOT NULL REFERENCES one_on_one_participant_pairs(id) ON DELETE CASCADE,
    confirmed_occurred_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(pair_id, meeting_id)
);

CREATE INDEX IF NOT EXISTS idx_one_on_one_series_pair_date
    ON one_on_one_series(pair_id, confirmed_occurred_at DESC);

CREATE TABLE IF NOT EXISTS one_on_one_private_notes (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    participant_slot TEXT NOT NULL CHECK (participant_slot IN ('participant_a', 'participant_b')),
    note_kind TEXT NOT NULL CHECK (note_kind IN ('agenda_draft', 'scratchpad')),
    content TEXT NOT NULL,
    shared_to_agenda INTEGER NOT NULL DEFAULT 0 CHECK (shared_to_agenda IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_one_on_one_private_notes_meeting_slot
    ON one_on_one_private_notes(meeting_id, participant_slot, note_kind, updated_at DESC);

CREATE TABLE IF NOT EXISTS one_on_one_live_markers (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    marker_kind TEXT NOT NULL CHECK (marker_kind IN (
        'feedback', 'support', 'growth', 'follow_up', 'return_later', 'deep_dive'
    )),
    elapsed_seconds INTEGER NOT NULL CHECK (elapsed_seconds >= 0),
    note TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_one_on_one_live_markers_meeting_time
    ON one_on_one_live_markers(meeting_id, elapsed_seconds, id);

CREATE TABLE IF NOT EXISTS one_on_one_recurring_topics (
    id INTEGER PRIMARY KEY,
    pair_id INTEGER NOT NULL REFERENCES one_on_one_participant_pairs(id) ON DELETE CASCADE,
    canonical_topic TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'closed', 'dismissed')),
    confirmed_by_user INTEGER NOT NULL DEFAULT 1 CHECK (confirmed_by_user IN (0, 1)),
    source_record_ids_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(pair_id, canonical_topic)
);

CREATE TABLE IF NOT EXISTS one_on_one_records (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    record_key TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN (
        'check_in', 'previous_follow_up', 'progress', 'challenge_support', 'feedback',
        'growth', 'decision', 'commitment', 'open_topic'
    )),
    payload TEXT NOT NULL,
    reviewed_payload TEXT,
    review_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (review_status IN ('pending', 'accepted', 'rejected')),
    carry_status TEXT NOT NULL DEFAULT 'open' CHECK (carry_status IN ('open', 'closed')),
    source_schema_version TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(meeting_id, record_key)
);

CREATE INDEX IF NOT EXISTS idx_one_on_one_records_meeting_status
    ON one_on_one_records(meeting_id, review_status, kind);

CREATE TABLE IF NOT EXISTS one_on_one_commitments (
    id INTEGER PRIMARY KEY,
    source_record_id INTEGER NOT NULL UNIQUE REFERENCES one_on_one_records(id) ON DELETE CASCADE,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    task TEXT NOT NULL,
    owner TEXT,
    due_date TEXT,
    status TEXT NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'done', 'cancelled', 'superseded')),
    evidence_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_one_on_one_commitments_meeting_status
    ON one_on_one_commitments(meeting_id, status, updated_at DESC);

CREATE TABLE IF NOT EXISTS one_on_one_audit_log (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL,
    payload TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_one_on_one_audit_meeting_created
    ON one_on_one_audit_log(meeting_id, created_at DESC);
