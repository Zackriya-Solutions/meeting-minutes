-- Per-meeting diarization preferences, set from the in-recording control pill.
--
-- diarization_enabled: NULL = default (enabled); 0 = the user turned speaker ID
-- off for this meeting. Gates only the AUTOMATIC post-meeting diarize job — the
-- manual "Detect speakers" button is an explicit request and always runs.
--
-- expected_speakers: NULL = automatic estimation; N >= 1 = user-provided hint.
-- Local engine: stage-2 centroid merging targets exactly N clusters. Cloud
-- (SaluteSpeech): forwarded as speaker_separation_options.count_of_speaker.
ALTER TABLE meetings ADD COLUMN diarization_enabled INTEGER;
ALTER TABLE meetings ADD COLUMN expected_speakers INTEGER;
