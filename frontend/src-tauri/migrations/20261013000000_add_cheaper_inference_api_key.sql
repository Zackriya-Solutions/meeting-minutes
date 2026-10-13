-- Add Cheaper Inference API key storage for the OpenAI-compatible summary provider
ALTER TABLE settings ADD COLUMN cheaperInferenceApiKey TEXT;
