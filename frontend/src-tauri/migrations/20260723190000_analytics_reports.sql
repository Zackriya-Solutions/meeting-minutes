CREATE TABLE IF NOT EXISTS analytics_reports (
    id TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    stage TEXT,
    stage_index INTEGER NOT NULL DEFAULT 0,
    total_stages INTEGER NOT NULL DEFAULT 0,
    model TEXT,
    artifacts TEXT,
    html_path TEXT,
    error TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_analytics_reports_meeting ON analytics_reports(meeting_id, created_at DESC);
