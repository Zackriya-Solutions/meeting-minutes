-- Migration: Add custom streaming transcription endpoint configuration
-- Stores JSON: {endpoint, apiKey, model, protocol, delayMs}
-- Used to connect meetily to a self-hosted realtime ASR websocket server
-- (e.g. vLLM serving Voxtral-Mini-Realtime) as a "custom" transcription provider.
ALTER TABLE transcript_settings ADD COLUMN customTranscriptionConfig TEXT;
