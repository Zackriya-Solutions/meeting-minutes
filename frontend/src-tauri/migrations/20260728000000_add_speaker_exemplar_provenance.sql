-- Provenance for saved voice exemplars.
--
-- Renaming a speaker away from a name used to leave behind the exemplar that
-- the earlier rename had contributed, permanently contaminating the profile
-- you corrected away from. Observed in the wild: profile "Alice" retained a
-- byte-identical copy of Camilia's voice vector after the speaker was
-- relabelled, and "Nick" likewise retained Ralf's. Two profiles holding the
-- same vector score 1.0 against each other and are reported as confusable.
--
-- Un-enrolling requires knowing which meeting and which local diarization
-- label an exemplar came from, which was never recorded. These columns record
-- it.
--
-- Additive and nullable: exemplars stored before this migration carry no
-- provenance and are simply skipped by the un-enroll path, which degrades to
-- the current behaviour rather than deleting the wrong row.
ALTER TABLE speaker_profile_embeddings ADD COLUMN source_meeting_id TEXT;
ALTER TABLE speaker_profile_embeddings ADD COLUMN source_label TEXT;

CREATE INDEX IF NOT EXISTS idx_speaker_profile_embeddings_source
    ON speaker_profile_embeddings (source_meeting_id, source_label);
