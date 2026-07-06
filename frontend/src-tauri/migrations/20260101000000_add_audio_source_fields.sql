-- Migration: Add audio source attribution fields for transcript segments
-- Values for audio_source: 'microphone', 'system', 'mixed', 'unknown'
-- This is source attribution, not speaker diarization. Do not use transcripts.speaker.
--
-- Columns are nullable with NO backfill, and that is intentional. Attribution is
-- computed live from the SEPARATE microphone and system energy during recording;
-- only the mixed audio is persisted to disk, so segments recorded before this
-- feature, imported from an external file, or re-transcribed from the saved mix
-- have no per-stream signal to attribute after the fact (recovering it from a
-- finished mix is source separation / diarization, which is out of scope). Such
-- rows keep audio_source = NULL and render with no source label in the UI, copy,
-- and summary input (see frontend/src/lib/transcriptSource.ts). NULL therefore
-- means "attribution unavailable", which is distinct from the 'unknown' value
-- (attribution ran but could not confidently pick a source).

ALTER TABLE transcripts ADD COLUMN audio_source TEXT CHECK (
    audio_source IS NULL
    OR audio_source IN ('microphone', 'system', 'mixed', 'unknown')
);
ALTER TABLE transcripts ADD COLUMN source_confidence REAL CHECK (
    source_confidence IS NULL
    OR (source_confidence >= 0.0 AND source_confidence <= 1.0)
);
