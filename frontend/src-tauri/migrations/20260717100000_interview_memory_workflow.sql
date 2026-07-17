-- Interview Memory V1: preparation, evidence review, privacy and multi-stage handoff.

ALTER TABLE meetings ADD COLUMN cloud_processing_allowed INTEGER NOT NULL DEFAULT 1
    CHECK (cloud_processing_allowed IN (0, 1));
ALTER TABLE meetings ADD COLUMN indexing_allowed INTEGER NOT NULL DEFAULT 1
    CHECK (indexing_allowed IN (0, 1));
ALTER TABLE meetings ADD COLUMN retention_days INTEGER
    CHECK (retention_days IS NULL OR retention_days BETWEEN 1 AND 3650);
ALTER TABLE meetings ADD COLUMN retention_expires_at TEXT;
ALTER TABLE meetings ADD COLUMN candidate_export_allowed INTEGER NOT NULL DEFAULT 0
    CHECK (candidate_export_allowed IN (0, 1));

-- Sensitive interviews fail closed. A user can opt in later per memory.
UPDATE meetings
SET cloud_processing_allowed = 0,
    indexing_allowed = 0
WHERE memory_type = 'interview' OR sensitivity = 'sensitive';

CREATE INDEX IF NOT EXISTS idx_meetings_privacy_retention
    ON meetings(indexing_allowed, retention_expires_at);

CREATE TABLE IF NOT EXISTS interview_configs (
    meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,
    candidate_name TEXT,
    role_title TEXT,
    interview_stage TEXT,
    interviewer_roles_json TEXT NOT NULL DEFAULT '[]',
    competencies_json TEXT NOT NULL DEFAULT '[]',
    success_criteria TEXT,
    question_plan_json TEXT NOT NULL DEFAULT '[]',
    glossary_json TEXT NOT NULL DEFAULT '[]',
    target_minutes INTEGER NOT NULL DEFAULT 60 CHECK (target_minutes BETWEEN 10 AND 240),
    candidate_questions_minutes INTEGER NOT NULL DEFAULT 10
        CHECK (candidate_questions_minutes BETWEEN 0 AND 60),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS interview_records (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    record_key TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN (
        'conversation_block', 'question_answer', 'evidence', 'case_exercise', 'open_question',
        'candidate_question', 'next_step'
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

CREATE INDEX IF NOT EXISTS idx_interview_records_meeting_status
    ON interview_records(meeting_id, review_status, kind);

CREATE TABLE IF NOT EXISTS interview_debriefs (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    reviewer_name TEXT NOT NULL,
    strengths TEXT NOT NULL DEFAULT '',
    concerns TEXT NOT NULL DEFAULT '',
    open_questions TEXT NOT NULL DEFAULT '',
    recommendation TEXT NOT NULL DEFAULT 'pending'
        CHECK (recommendation IN ('pending', 'advance', 'hold', 'decline')),
    submitted_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(meeting_id, reviewer_name)
);

CREATE TABLE IF NOT EXISTS interview_tracks (
    id INTEGER PRIMARY KEY,
    candidate_name TEXT NOT NULL,
    role_title TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'completed', 'archived')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS interview_track_meetings (
    track_id INTEGER NOT NULL REFERENCES interview_tracks(id) ON DELETE CASCADE,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    stage_order INTEGER NOT NULL CHECK (stage_order BETWEEN 1 AND 100),
    stage_name TEXT,
    PRIMARY KEY(track_id, meeting_id),
    UNIQUE(track_id, stage_order)
);

CREATE INDEX IF NOT EXISTS idx_interview_track_meetings_meeting
    ON interview_track_meetings(meeting_id);

CREATE TABLE IF NOT EXISTS interview_audit_log (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL,
    payload TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_interview_audit_meeting_created
    ON interview_audit_log(meeting_id, created_at DESC);
