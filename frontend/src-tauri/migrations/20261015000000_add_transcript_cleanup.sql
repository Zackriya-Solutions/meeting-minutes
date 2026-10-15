-- Optional post-transcription cleanup. Off until the user turns it on.
ALTER TABLE settings ADD COLUMN cleanupEnabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE settings ADD COLUMN cleanupProvider TEXT;
ALTER TABLE settings ADD COLUMN cleanupModel TEXT;
ALTER TABLE settings ADD COLUMN cleanupOllamaEndpoint TEXT;
ALTER TABLE settings ADD COLUMN cleanupPrompt TEXT NOT NULL DEFAULT '';

-- Speech-to-text text kept so a later cleanup starts from the recording, not from a previous correction.
ALTER TABLE transcripts ADD COLUMN transcript_original TEXT;
