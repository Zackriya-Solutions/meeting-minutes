-- Persist the editable system prompt used for final summary generation.
ALTER TABLE settings ADD COLUMN summarySystemPrompt TEXT;
