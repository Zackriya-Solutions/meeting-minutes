-- Deepgram diarization became opt-in (default off) when realtime capture moved
-- to separate Mic-In / Speaker-Out channels. Every settings save used to write
-- the old default (on) alongside whatever field changed, so a stored 1 does not
-- mean the user chose diarization. Clear it once; NULL now reads as off.
-- DO NOT edit 20260101000000_add_deepgram_options.sql instead: sqlx checksums
-- applied migrations, and its "defaults to on" note is superseded by this one.
UPDATE transcript_settings SET deepgramDiarize = NULL;
