-- Persist the selected MiniMax OpenAI-compatible endpoint region.
ALTER TABLE settings ADD COLUMN minimaxRegion TEXT NOT NULL DEFAULT 'global';
