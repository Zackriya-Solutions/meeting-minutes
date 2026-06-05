-- Add a per-transcription-config flag for live transcript generation.
-- Default is disabled so recordings can save audio without requiring a loaded STT model.
ALTER TABLE transcript_settings
ADD COLUMN realtimeTranscriptionEnabled BOOLEAN NOT NULL DEFAULT FALSE;
