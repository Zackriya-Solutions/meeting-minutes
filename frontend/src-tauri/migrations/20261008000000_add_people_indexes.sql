-- Voice matching and person edits look up speakers, suggestions and rejections by person.
CREATE INDEX IF NOT EXISTS idx_meeting_speakers_person ON meeting_speakers(person_id);
CREATE INDEX IF NOT EXISTS idx_meeting_speakers_suggested_person ON meeting_speakers(suggested_person_id);
CREATE INDEX IF NOT EXISTS idx_speaker_rejections_person ON speaker_rejections(person_id);
