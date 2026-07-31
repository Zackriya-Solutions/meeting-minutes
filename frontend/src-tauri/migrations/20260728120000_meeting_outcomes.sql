CREATE TABLE IF NOT EXISTS meeting_outcomes (
    id TEXT PRIMARY KEY,
    meeting_id TEXT REFERENCES meetings(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft'
        CHECK (status IN ('draft', 'queued', 'running', 'completed', 'failed')),
    artifact TEXT NOT NULL DEFAULT '{}',
    job_id INTEGER REFERENCES jobs(id) ON DELETE SET NULL,
    error TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_meeting_outcomes_meeting
    ON meeting_outcomes(meeting_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_meeting_outcomes_session
    ON meeting_outcomes(session_id, created_at DESC);
