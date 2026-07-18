-- Evidence extraction gained reviewable title and unassigned-address candidates after the
-- original table was shipped. Keep the vocabulary in data rather than a brittle CHECK so a
-- future evidence kind can be added with an INSERT instead of rebuilding this table again.
CREATE TABLE speaker_name_evidence_kinds (
    kind TEXT PRIMARY KEY,
    label_key TEXT NOT NULL
);

INSERT INTO speaker_name_evidence_kinds(kind, label_key) VALUES
    ('self_introduction', 'Self introduction'),
    ('explicit_introduction', 'Explicit introduction'),
    ('direct_address', 'Direct address'),
    ('direct_address_unassigned', 'Name mentioned in an address'),
    ('meeting_title', 'Name mentioned in the meeting title');

-- Rebuild both related tables together. speaker_aliases references candidate ids, so copying it
-- before dropping the old candidate table preserves accepted aliases and their provenance.
CREATE TABLE speaker_name_candidates_v2 (
    id INTEGER PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    proposed_speaker_id INTEGER REFERENCES speakers(id) ON DELETE SET NULL,
    proposed_speaker_key INTEGER NOT NULL,
    candidate_text TEXT,
    normalized_name TEXT,
    candidate_hash TEXT NOT NULL,
    evidence_kind TEXT NOT NULL REFERENCES speaker_name_evidence_kinds(kind),
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

INSERT INTO speaker_name_candidates_v2 (
    id, meeting_id, proposed_speaker_id, proposed_speaker_key, candidate_text,
    normalized_name, candidate_hash, evidence_kind, evidence_quote, evidence_start_ms,
    confidence, occurrence_count, status, created_at, updated_at
)
SELECT
    id, meeting_id, proposed_speaker_id, proposed_speaker_key, candidate_text,
    normalized_name, candidate_hash, evidence_kind, evidence_quote, evidence_start_ms,
    confidence, occurrence_count, status, created_at, updated_at
FROM speaker_name_candidates;

CREATE TABLE speaker_aliases_v2 (
    id INTEGER PRIMARY KEY,
    speaker_id INTEGER NOT NULL REFERENCES speakers(id) ON DELETE CASCADE,
    alias TEXT NOT NULL,
    normalized_alias TEXT NOT NULL,
    source_candidate_id INTEGER REFERENCES speaker_name_candidates_v2(id) ON DELETE SET NULL,
    is_confirmed INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO speaker_aliases_v2 (
    id, speaker_id, alias, normalized_alias, source_candidate_id, is_confirmed, created_at
)
SELECT
    id, speaker_id, alias, normalized_alias, source_candidate_id, is_confirmed, created_at
FROM speaker_aliases;

DROP TABLE speaker_aliases;
DROP TABLE speaker_name_candidates;
ALTER TABLE speaker_name_candidates_v2 RENAME TO speaker_name_candidates;
ALTER TABLE speaker_aliases_v2 RENAME TO speaker_aliases;

CREATE INDEX idx_speaker_name_candidates_meeting
    ON speaker_name_candidates(meeting_id, status);
CREATE INDEX idx_speaker_aliases_speaker ON speaker_aliases(speaker_id);
CREATE UNIQUE INDEX idx_speaker_aliases_unique
    ON speaker_aliases(speaker_id, normalized_alias);
