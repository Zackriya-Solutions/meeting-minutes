-- Track processing per imported take, not only on the canonical audio identity.
-- This keeps deliberate raw/denoised comparisons possible while making each
-- (audio hash, processing mode) import idempotent.
ALTER TABLE meeting_audio_identities ADD COLUMN denoise_applied INTEGER
    CHECK (denoise_applied IS NULL OR denoise_applied IN (0, 1));

UPDATE meeting_audio_identities
SET denoise_applied = (
    SELECT ai.denoise_applied
    FROM audio_identities ai
    WHERE ai.sha256 = meeting_audio_identities.sha256
)
WHERE role = 'canonical';

CREATE UNIQUE INDEX IF NOT EXISTS idx_meeting_audio_identity_processing_variant
    ON meeting_audio_identities(sha256, denoise_applied)
    WHERE denoise_applied IS NOT NULL;
