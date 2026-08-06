-- Repair transcript settings the onboarding path wrote after the collapse.
--
-- 20260806000000 mapped rows by provider, so it only caught provider IN
-- ('localWhisper', 'whisper', 'parakeet'). Onboarding kept writing
-- provider='parakeet' / model='parakeet-tdt-0.6b-v3-int8' after that migration
-- ran, and any row created since then still holds a model name transcribe.cpp
-- cannot resolve — the settings UI shows it, and loading it fails.
--
-- Match on the model name rather than the provider this time: that is the value
-- that is actually unresolvable, whichever provider string it ended up beside.
UPDATE transcript_settings
SET provider = 'local',
    model = 'nemotron-3.5-asr-streaming-0.6b-q8'
WHERE model = 'parakeet-tdt-0.6b-v3-int8'
   OR provider = 'parakeet';

-- The catalog now names every row <variant>-<quant>, because most variants ship
-- more than one quantization. The two Moonshine entries that predate that were
-- bare variant names; keep the user on the same model instead of resetting them.
UPDATE transcript_settings
SET model = model || '-q8'
WHERE model IN ('moonshine-streaming-small', 'moonshine-streaming-tiny');
