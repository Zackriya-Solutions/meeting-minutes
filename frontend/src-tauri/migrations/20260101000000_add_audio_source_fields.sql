-- Migration: Add audio source attribution fields for transcript segments
-- Values for audio_source: 'microphone', 'system', 'mixed', 'unknown'
-- This is source attribution, not speaker diarization. Do not use transcripts.speaker.

ALTER TABLE transcripts ADD COLUMN audio_source TEXT CHECK (
    audio_source IS NULL
    OR audio_source IN ('microphone', 'system', 'mixed', 'unknown')
);
ALTER TABLE transcripts ADD COLUMN source_confidence REAL CHECK (
    source_confidence IS NULL
    OR (source_confidence >= 0.0 AND source_confidence <= 1.0)
);
