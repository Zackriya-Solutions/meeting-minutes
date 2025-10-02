-- Add openRouterApiKey column to settings table if it doesn't exist
PRAGMA foreign_keys=off;

-- Create a new table with the new column
CREATE TABLE IF NOT EXISTS settings_new (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    whisperModel TEXT NOT NULL,
    groqApiKey TEXT,
    openaiApiKey TEXT,
    anthropicApiKey TEXT,
    ollamaApiKey TEXT,
    openRouterApiKey TEXT
);

-- Copy data from old table to new table
INSERT INTO settings_new 
SELECT *, NULL as openRouterApiKey 
FROM settings;

-- Drop the old table
DROP TABLE settings;

-- Rename new table to original name
ALTER TABLE settings_new RENAME TO settings;

PRAGMA foreign_keys=on;
