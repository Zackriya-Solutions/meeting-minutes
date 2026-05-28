-- Persist editable prompts used for long-transcript intermediate summarization.
ALTER TABLE settings ADD COLUMN summaryChunkPrompt TEXT;
ALTER TABLE settings ADD COLUMN summaryCombinePrompt TEXT;
