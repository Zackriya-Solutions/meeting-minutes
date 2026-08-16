-- Why a transcript line ended up without a speaker.
--
-- Attribution declines for two different reasons — the diarizer barely saw the line, or
-- several voices share it — and the remedies have nothing in common: the first is an audio
-- problem, the second is a segmentation problem. Neither the reason nor the numbers behind
-- it were recorded, so the size of each population was unknown and every proposal about
-- unattributed lines was unfalsifiable.
--
-- One row per refused segment per diarization run. Rows are replaced when the meeting is
-- diarized again, and follow the meeting and the transcript when either is deleted.
CREATE TABLE IF NOT EXISTS unattributed_segments (
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
    PRIMARY KEY (transcript_id)
);

CREATE INDEX IF NOT EXISTS idx_unattributed_segments_meeting
    ON unattributed_segments(meeting_id, reason);
