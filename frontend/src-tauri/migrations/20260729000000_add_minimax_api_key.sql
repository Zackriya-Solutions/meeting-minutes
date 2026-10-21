-- Add MiniMax API key storage for the OpenAI-compatible summary provider
ALTER TABLE settings ADD COLUMN minimaxApiKey TEXT;
