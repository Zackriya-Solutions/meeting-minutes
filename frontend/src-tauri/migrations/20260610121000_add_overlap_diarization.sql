-- Overlap-aware diarization metadata.
-- Transcript columns are nullable for backward compatibility with existing
-- segment-level transcripts. overlap_speaker_ids stores a JSON string.
ALTER TABLE transcripts ADD COLUMN attribution_source TEXT;
ALTER TABLE transcripts ADD COLUMN overlap_region_id TEXT;
ALTER TABLE transcripts ADD COLUMN overlap_speaker_ids TEXT;
ALTER TABLE transcripts ADD COLUMN overlap_start_time REAL;
ALTER TABLE transcripts ADD COLUMN overlap_end_time REAL;
ALTER TABLE transcripts ADD COLUMN overlap_confidence REAL;

CREATE TABLE IF NOT EXISTS overlap_regions (
    meeting_id TEXT NOT NULL,
    id TEXT NOT NULL,
    start_ms INTEGER NOT NULL,
    end_ms INTEGER NOT NULL,
    speaker_ids TEXT NOT NULL,
    confidence REAL,
    estimated_speaker_count INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL,
    PRIMARY KEY (meeting_id, id),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
