-- Persist editable system prompts used during long-transcript intermediate summarization.
ALTER TABLE settings ADD COLUMN summaryChunkSystemPrompt TEXT;
ALTER TABLE settings ADD COLUMN summaryCombineSystemPrompt TEXT;
