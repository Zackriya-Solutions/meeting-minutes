-- Wave 15 PR-45c-ui: per-user hot-word list for local Whisper
-- Stores a comma/space-separated list of vocabulary biases that gets
-- forwarded to whisper.cpp as `params.set_initial_prompt`. Empty / NULL
-- means no bias is applied (legacy behavior).
ALTER TABLE transcript_settings ADD COLUMN hotwords TEXT;
