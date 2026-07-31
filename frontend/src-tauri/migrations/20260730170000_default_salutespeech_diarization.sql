-- SaluteSpeech becomes the default speaker-separation engine. The local pyannote-style
-- models remain an offline fallback and are no longer required before starting a run.
-- Preserve an explicit local-only privacy choice: cloud configuration also fails closed
-- at runtime, but keeping `local` here avoids attempting the cloud branch at all.

INSERT OR IGNORE INTO app_settings_kv(key, value, updated_at)
VALUES('diarization.provider', 'salutespeech', datetime('now'));

UPDATE app_settings_kv
SET value = CASE
        WHEN EXISTS (
            SELECT 1
            FROM app_settings_kv AS privacy
            WHERE privacy.key = 'privacy.local_only'
              AND lower(trim(privacy.value)) IN ('true', '1')
        ) THEN 'local'
        ELSE 'salutespeech'
    END,
    updated_at = datetime('now')
WHERE key = 'diarization.provider';
