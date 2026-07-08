-- deepgramMipOptOut: 1/0 toggle for Deepgram's Model Improvement Program opt-out
-- (sends mip_opt_out=true). Meetily is privacy-first, so the effective default is
-- ON (opt out) when the column is NULL.
ALTER TABLE transcript_settings ADD COLUMN deepgramMipOptOut INTEGER;
