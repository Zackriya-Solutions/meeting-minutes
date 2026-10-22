-- Migration: Gemini Live transcription settings (fork addition)
-- Kept in its own table so upstream changes to transcript_settings merge cleanly.

CREATE TABLE IF NOT EXISTS gemini_live_settings (
    id TEXT PRIMARY KEY,
    api_key TEXT,
    -- 'roman' (Hinglish in Latin script), 'devanagari', or 'auto'
    language_mode TEXT NOT NULL DEFAULT 'roman',
    -- JSON array of terms
    custom_vocabulary TEXT NOT NULL DEFAULT '[]',
    -- 'VERBATIM' or 'SMART'
    transcription_mode TEXT NOT NULL DEFAULT 'VERBATIM',
    -- 'parakeet' or 'localWhisper'
    fallback_provider TEXT NOT NULL DEFAULT 'parakeet'
);
