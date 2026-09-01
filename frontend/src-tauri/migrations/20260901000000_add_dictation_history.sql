CREATE TABLE IF NOT EXISTS dictation_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    phase TEXT NOT NULL,
    raw_text TEXT,
    final_text TEXT,
    failure_code TEXT,
    failure_message TEXT,
    retryable INTEGER NOT NULL DEFAULT 0,
    audio_path TEXT,
    target_process TEXT,
    delivery_method TEXT,
    started_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    completed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_dictation_sessions_started_at
    ON dictation_sessions(started_at DESC);
