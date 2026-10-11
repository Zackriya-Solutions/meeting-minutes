-- Speaker identification, merges and row reassignment read a meeting's transcripts by speaker.
CREATE INDEX IF NOT EXISTS idx_transcripts_meeting_speaker ON transcripts(meeting_id, speaker);
