-- Deepgram provider-specific knobs surfaced in Transcription settings.
-- deepgramKeyterm: newline/comma-separated keyterm-prompting phrases (nova-3) /
--                  keyword boosts (nova-2).
-- deepgramDiarize: 1/0 toggle for speaker diarization (defaults to on when NULL).
ALTER TABLE transcript_settings ADD COLUMN deepgramKeyterm TEXT;
ALTER TABLE transcript_settings ADD COLUMN deepgramDiarize INTEGER;
