-- Exact audio identity for idempotent imports and a non-destructive legacy audit.
--
-- `audio_identities` owns the unique content hash and points to the meeting that
-- new imports should reuse. Existing duplicate meetings remain intact and are
-- linked as `duplicate_candidate` until a user reviews them.
CREATE TABLE IF NOT EXISTS audio_identities (
    sha256 TEXT PRIMARY KEY
        CHECK (length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-f]*'),
    canonical_meeting_id TEXT NOT NULL UNIQUE
        REFERENCES meetings(id) ON DELETE RESTRICT,
    byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
    duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    verified_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS meeting_audio_identities (
    meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,
    sha256 TEXT NOT NULL REFERENCES audio_identities(sha256) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('canonical', 'duplicate_candidate')),
    detected_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_meeting_audio_identities_sha
    ON meeting_audio_identities(sha256, role, meeting_id);

CREATE TABLE IF NOT EXISTS audio_duplicate_reviews (
    duplicate_meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,
    canonical_meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    sha256 TEXT NOT NULL REFERENCES audio_identities(sha256) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'kept_separate', 'merged')),
    detected_at TEXT NOT NULL DEFAULT (datetime('now')),
    resolved_at TEXT,
    CHECK (duplicate_meeting_id <> canonical_meeting_id)
);

CREATE INDEX IF NOT EXISTS idx_audio_duplicate_reviews_status
    ON audio_duplicate_reviews(status, detected_at, duplicate_meeting_id);
