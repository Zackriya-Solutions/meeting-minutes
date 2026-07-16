ALTER TABLE transcript_settings ADD COLUMN diarizationEnabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE transcript_settings ADD COLUMN diarizationProvider TEXT NOT NULL DEFAULT 'local';
ALTER TABLE transcript_settings ADD COLUMN azureSpeechKey TEXT;
ALTER TABLE transcript_settings ADD COLUMN azureSpeechRegion TEXT;
