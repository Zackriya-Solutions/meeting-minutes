-- Move audio-LLM transcription from Ollama to the bundled sidecar.
--
-- Gemma 4 E2B/E4B transcription used to go over HTTP to a separately installed
-- Ollama. The app already ships a llama.cpp sidecar for summaries, and the same
-- crate exposes mtmd, so the identical model now runs in-process — no second
-- install, no localhost:11434, no second copy of the weights.
--
-- The model tag is unchanged ('gemma4:e2b' / 'gemma4:e4b') because the built-in
-- catalog reuses those names; only the provider moves. Anything else that was
-- stored under the 'ollama' transcript provider had no audio-capable model behind
-- it, so it lands on the default.
--
-- Summaries are untouched: 'ollama' remains a valid summary provider for users
-- who already run their own model library.
UPDATE transcript_settings
SET provider = 'builtin-ai'
WHERE provider = 'ollama'
  AND model IN ('gemma4:e2b', 'gemma4:e4b');

UPDATE transcript_settings
SET provider = 'builtin-ai',
    model = 'gemma4:e4b'
WHERE provider = 'ollama';
