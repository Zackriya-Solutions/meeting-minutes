-- Names heard in a transcript are untrusted suggestions. They never rename a speaker without
-- explicit user confirmation, and rejected unsafe strings are retained only as salted hashes.
CREATE TABLE IF NOT EXISTS speaker_name_candidates (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    proposed_speaker_id INTEGER REFERENCES speakers(id) ON DELETE SET NULL,
    proposed_speaker_key INTEGER NOT NULL,
    candidate_text TEXT,
    normalized_name TEXT,
    candidate_hash TEXT NOT NULL,
    evidence_kind TEXT NOT NULL CHECK (evidence_kind IN (
        'self_introduction', 'explicit_introduction', 'direct_address'
    )),
    evidence_quote TEXT,
    evidence_start_ms INTEGER,
    confidence REAL NOT NULL CHECK (confidence >= 0 AND confidence <= 1),
    occurrence_count INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'accepted', 'rejected')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(meeting_id, candidate_hash, proposed_speaker_key, evidence_kind)
);
CREATE INDEX IF NOT EXISTS idx_speaker_name_candidates_meeting
    ON speaker_name_candidates(meeting_id, status);

CREATE TABLE IF NOT EXISTS speaker_aliases (
    id INTEGER PRIMARY KEY,
    speaker_id INTEGER NOT NULL REFERENCES speakers(id) ON DELETE CASCADE,
    alias TEXT NOT NULL,
    normalized_alias TEXT NOT NULL,
    source_candidate_id INTEGER REFERENCES speaker_name_candidates(id) ON DELETE SET NULL,
    is_confirmed INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_speaker_aliases_speaker ON speaker_aliases(speaker_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_speaker_aliases_unique
    ON speaker_aliases(speaker_id, normalized_alias);

CREATE TABLE IF NOT EXISTS rejected_speaker_name_fingerprints (
    candidate_hash TEXT PRIMARY KEY,
    reason TEXT NOT NULL,
    occurrence_count INTEGER NOT NULL DEFAULT 1,
    last_seen_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- A user's rejection is a judgment about one proposed link, not a global ban on the
-- name. Keep it scoped without retaining the rejected raw text.
CREATE TABLE IF NOT EXISTS rejected_speaker_name_candidate_instances (
    meeting_id TEXT NOT NULL,
    candidate_hash TEXT NOT NULL,
    proposed_speaker_key INTEGER NOT NULL,
    evidence_kind TEXT NOT NULL,
    occurrence_count INTEGER NOT NULL DEFAULT 1,
    last_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (meeting_id, candidate_hash, proposed_speaker_key, evidence_kind)
);
