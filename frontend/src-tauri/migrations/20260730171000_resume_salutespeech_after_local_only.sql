-- Keep SaluteSpeech selected while privacy.local_only enforces the network boundary at
-- runtime. This lets cloud diarization resume automatically if local-only is disabled.

UPDATE app_settings_kv
SET value = 'salutespeech', updated_at = datetime('now')
WHERE key = 'diarization.provider';
