-- deepgramLanguage: Deepgram-specific language mode, decoupled from the shared
-- global language preference (which is Whisper/Parakeet-oriented). Values:
--   'multi'  -> language=multi (nova-3 code-switching)
--   'detect' -> detect_language=true (auto-detect one language)
--   ISO code -> language=<code>
-- NULL/'' falls back to the nova-3 default (multi). Kept separate so Deepgram-only
-- values never leak into the Whisper/Parakeet language path.
ALTER TABLE transcript_settings ADD COLUMN deepgramLanguage TEXT;
