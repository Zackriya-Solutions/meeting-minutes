-- Local models become the only supported defaults (2026-07-20). Measured on a real
-- 31-min 7-speaker meeting: SaluteSpeech cloud ASR matched 80.4% of reference words
-- (GigaAM: 92.4% on identical segmentation) and its diarization found 4 of 7 speakers
-- at 68.8% agreement (local engine: 7/7 at 92.5%). The Settings UI no longer offers
-- SaluteSpeech; installs previously configured or auto-migrated onto it move back here.

UPDATE transcript_settings
SET provider = 'gigaam', model = 'gigaam-v3-e2e-rnnt-fp32'
WHERE id = '1' AND provider = 'salutespeech';

UPDATE app_settings_kv
SET value = 'local', updated_at = datetime('now')
WHERE key = 'diarization.provider' AND value = 'salutespeech';
