-- Privacy-safe lifecycle for automatically detected recording sessions.
-- Raw process names, window titles, URLs, and transcript text never enter these tables.

CREATE TABLE IF NOT EXISTS capture_sessions (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL CHECK (source IN ('microphone_activity', 'native_process')),
    client_kinds TEXT NOT NULL DEFAULT '[]',
    capture_mode TEXT NOT NULL DEFAULT 'auto'
        CHECK (capture_mode IN ('manual', 'assisted', 'auto')),
    detected_at TEXT NOT NULL DEFAULT (datetime('now')),
    capture_started_at TEXT,
    recording_started_at TEXT,
    signal_ended_at TEXT,
    ended_at TEXT,
    status TEXT NOT NULL DEFAULT 'start_requested' CHECK (status IN (
        'candidate', 'start_requested', 'recording', 'stop_requested',
        'saved', 'discarded', 'failed', 'recovered'
    )),
    failure_reason TEXT,
    end_reason TEXT,
    sample_rate INTEGER CHECK (sample_rate IS NULL OR sample_rate > 0),
    microphone_present INTEGER NOT NULL DEFAULT 1 CHECK (microphone_present IN (0, 1)),
    system_audio_present INTEGER NOT NULL DEFAULT 0 CHECK (system_audio_present IN (0, 1)),
    speech_duration_ms INTEGER NOT NULL DEFAULT 0 CHECK (speech_duration_ms >= 0),
    silence_duration_ms INTEGER NOT NULL DEFAULT 0 CHECK (silence_duration_ms >= 0),
    dropped_chunks INTEGER NOT NULL DEFAULT 0 CHECK (dropped_chunks >= 0),
    retention_expires_at TEXT,
    meeting_id TEXT REFERENCES meetings(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_capture_sessions_status_detected
    ON capture_sessions(status, detected_at DESC);

CREATE INDEX IF NOT EXISTS idx_capture_sessions_meeting
    ON capture_sessions(meeting_id)
    WHERE meeting_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS meeting_windows (
    id INTEGER PRIMARY KEY,
    capture_session_id TEXT NOT NULL REFERENCES capture_sessions(id) ON DELETE CASCADE,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    start_offset_ms INTEGER NOT NULL DEFAULT 0 CHECK (start_offset_ms >= 0),
    end_offset_ms INTEGER CHECK (end_offset_ms IS NULL OR end_offset_ms >= start_offset_ms),
    suggested_start_ms INTEGER CHECK (suggested_start_ms IS NULL OR suggested_start_ms >= 0),
    suggested_end_ms INTEGER CHECK (
        suggested_end_ms IS NULL OR
        (suggested_end_ms >= COALESCE(suggested_start_ms, start_offset_ms))
    ),
    confirmed_start_ms INTEGER CHECK (confirmed_start_ms IS NULL OR confirmed_start_ms >= 0),
    confirmed_end_ms INTEGER CHECK (
        confirmed_end_ms IS NULL OR
        (confirmed_end_ms >= COALESCE(confirmed_start_ms, start_offset_ms))
    ),
    boundary_source TEXT NOT NULL DEFAULT 'call_signal' CHECK (boundary_source IN (
        'call_signal', 'content_window', 'manual'
    )),
    confidence REAL CHECK (confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0)),
    is_confirmed INTEGER NOT NULL DEFAULT 0 CHECK (is_confirmed IN (0, 1)),
    review_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (review_status IN ('pending', 'accepted', 'rejected', 'superseded')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_meeting_windows_meeting
    ON meeting_windows(meeting_id);
CREATE INDEX IF NOT EXISTS idx_meeting_windows_capture_meeting
    ON meeting_windows(capture_session_id, meeting_id, start_offset_ms);

-- Aggregated state transitions only. Raw process names, window titles, URLs and
-- frame-level activity are intentionally excluded.
CREATE TABLE IF NOT EXISTS capture_observations (
    id INTEGER PRIMARY KEY,
    capture_session_id TEXT NOT NULL REFERENCES capture_sessions(id) ON DELETE CASCADE,
    offset_ms INTEGER NOT NULL CHECK (offset_ms >= 0),
    signal_kind TEXT NOT NULL CHECK (signal_kind IN (
        'microphone', 'system_audio', 'speech', 'client', 'recording', 'device'
    )),
    signal_state TEXT NOT NULL,
    source TEXT NOT NULL,
    confidence REAL CHECK (confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0)),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(capture_session_id, offset_ms, signal_kind, signal_state)
);
CREATE INDEX IF NOT EXISTS idx_capture_observations_session_offset
    ON capture_observations(capture_session_id, offset_ms);

CREATE TABLE IF NOT EXISTS capture_retention_policy (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    unpromoted_retention_minutes INTEGER NOT NULL DEFAULT 15
        CHECK (unpromoted_retention_minutes BETWEEN 1 AND 1440),
    saved_audio_retention_days INTEGER
        CHECK (saved_audio_retention_days IS NULL OR
               saved_audio_retention_days BETWEEN 1 AND 3650),
    local_only INTEGER NOT NULL DEFAULT 1 CHECK (local_only IN (0, 1)),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT OR IGNORE INTO capture_retention_policy(id) VALUES(1);
