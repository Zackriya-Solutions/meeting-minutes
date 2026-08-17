-- Google Calendar integration: connected account, synced events, and a link back to meetings
CREATE TABLE IF NOT EXISTS calendar_accounts (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    provider TEXT NOT NULL DEFAULT 'google',
    email TEXT NOT NULL,
    access_token TEXT NOT NULL,
    refresh_token TEXT NOT NULL,
    token_expires_at TEXT NOT NULL,
    scope TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'connected',
    connected_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS calendar_events (
    id TEXT PRIMARY KEY,
    calendar_account_id INTEGER NOT NULL REFERENCES calendar_accounts(id) ON DELETE CASCADE,
    title TEXT,
    start_time TEXT NOT NULL,
    end_time TEXT NOT NULL,
    meeting_url TEXT,
    meeting_provider TEXT,
    raw_json TEXT,
    triggered_start_at TEXT,
    triggered_stop_at TEXT,
    linked_meeting_id TEXT REFERENCES meetings(id) ON DELETE SET NULL,
    synced_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_calendar_events_start_time ON calendar_events(start_time);

ALTER TABLE meetings ADD COLUMN calendar_event_id TEXT REFERENCES calendar_events(id);
