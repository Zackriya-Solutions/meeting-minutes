-- Count distinct unsafe transcript observations rather than repeated scans.
-- Retain only salted fingerprints and structural evidence coordinates.
CREATE TABLE IF NOT EXISTS rejected_speaker_name_observations (
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    candidate_hash TEXT NOT NULL,
    evidence_start_ms INTEGER NOT NULL,
    evidence_kind TEXT NOT NULL,
    PRIMARY KEY (meeting_id, candidate_hash, evidence_start_ms, evidence_kind)
);
