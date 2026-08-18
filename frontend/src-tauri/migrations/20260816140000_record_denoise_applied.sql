-- Record how an imported file was processed, so the app can answer "was this
-- meeting denoised?" after the fact — and so flipping the denoise switch and
-- importing the same file again is a different run, not a refused duplicate.
--
-- NULL means "imported before this column existed": unknown, not "no".
ALTER TABLE audio_identities ADD COLUMN denoise_applied INTEGER
    CHECK (denoise_applied IS NULL OR denoise_applied IN (0, 1));
