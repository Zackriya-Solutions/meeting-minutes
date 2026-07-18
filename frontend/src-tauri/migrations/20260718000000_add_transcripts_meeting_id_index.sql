-- Add index for per-meeting transcript lookups; composite with audio_start_time
-- also serves the paginated ORDER BY audio_start_time query
CREATE INDEX IF NOT EXISTS idx_transcripts_meeting_id ON transcripts(meeting_id, audio_start_time);
