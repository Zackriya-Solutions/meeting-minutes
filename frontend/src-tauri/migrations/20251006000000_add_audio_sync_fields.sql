-- Migration: Add audio synchronization fields for playback support
-- This migration adds:
--   1. folder_path to meetings table (for new folder-based organization)
--   2. audio timing fields to transcripts table (for audio-transcript sync)

-- Add folder_path column to meetings table
-- This supports the new folder-based file organization structure
ALTER TABLE meetings ADD COLUMN folder_path TEXT;

-- Add audio timing columns to transcripts table
-- These enable precise audio-transcript synchronization for playback:
--   - audio_start_time: Seconds from recording start (e.g., 125.3)
--   - audio_end_time: Seconds from recording start (e.g., 128.6)
--   - duration: Segment duration in seconds (e.g., 3.3)
ALTER TABLE transcripts ADD COLUMN audio_start_time REAL;
ALTER TABLE transcripts ADD COLUMN audio_end_time REAL;
ALTER TABLE transcripts ADD COLUMN duration REAL;
