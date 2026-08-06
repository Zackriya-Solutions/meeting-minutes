-- Collapse the local transcription providers onto one engine.
--
-- Whisper (whisper-rs) and Parakeet (ONNX Runtime) were replaced by a single
-- transcribe.cpp engine that runs both families from GGUF. The provider string
-- no longer selects an engine, so the three historical local values collapse to
-- 'local'.
--
-- The model name is reset rather than mapped: transcribe.cpp reads GGUF only, so
-- no previously downloaded model file is reusable, and the catalog is now
-- streaming-native models that have no old-name equivalent. Users re-download
-- from settings; the app reports the model as Missing and the existing download
-- flow takes over.
UPDATE transcript_settings
SET provider = 'local',
    model = 'nemotron-3.5-asr-streaming-0.6b-q8'
WHERE provider IN ('localWhisper', 'whisper', 'parakeet');
