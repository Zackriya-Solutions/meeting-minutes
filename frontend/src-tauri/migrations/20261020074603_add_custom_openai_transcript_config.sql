-- Migration: Add Custom OpenAI-Compatible Transcription Config

-- This column stores: {endpoint, apiKey, model}
ALTER TABLE transcript_settings ADD COLUMN customOpenAIConfig TEXT;
