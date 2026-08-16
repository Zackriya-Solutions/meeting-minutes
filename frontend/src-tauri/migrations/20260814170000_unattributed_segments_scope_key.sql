-- Key the refusal record by (meeting, transcript) rather than by transcript alone.
--
-- A transcript id already belongs to exactly one meeting, so the single-column key was not
-- reachable in practice — but the table's whole purpose is a per-meeting count, and the key
-- should say so rather than leave the scope to an invariant held elsewhere. Recreated
-- rather than altered: SQLite cannot change a primary key in place, and the table holds
-- diagnostics regenerated on the next diarization run, so nothing of the user's is at stake.
DROP TABLE IF EXISTS unattributed_segments;

CREATE TABLE unattributed_segments (
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    transcript_id TEXT NOT NULL REFERENCES transcripts(id) ON DELETE CASCADE,
    reason TEXT NOT NULL CHECK (reason IN ('no_coverage', 'low_coverage', 'contested')),
    -- Share of the line covered by any diarized turn, 0..=1.
    coverage_ratio REAL NOT NULL,
    -- Share of that covered time owned by the leading cluster, and by the runner-up.
    top_ratio REAL NOT NULL,
    runner_up_ratio REAL NOT NULL,
    duration_ms INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (meeting_id, transcript_id)
);

CREATE INDEX IF NOT EXISTS idx_unattributed_segments_meeting
    ON unattributed_segments(meeting_id, reason);
