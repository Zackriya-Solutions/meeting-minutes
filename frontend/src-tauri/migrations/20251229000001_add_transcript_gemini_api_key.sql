-- Add Gemini API key storage for hosted transcription providers.

ALTER TABLE transcript_settings ADD COLUMN geminiApiKey TEXT;
