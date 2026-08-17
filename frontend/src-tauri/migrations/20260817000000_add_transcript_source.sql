-- Add source column to transcripts table to persist which audio channel a segment came from
-- Values: 'Você' for microphone, 'Outros' for system audio; NULL for legacy rows

ALTER TABLE transcripts ADD COLUMN source TEXT;
