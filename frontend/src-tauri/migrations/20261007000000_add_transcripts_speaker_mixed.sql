-- 1 when Identify kept a row with a speaker change whole under its majority speaker.
ALTER TABLE transcripts ADD COLUMN speaker_mixed INTEGER NOT NULL DEFAULT 0;
