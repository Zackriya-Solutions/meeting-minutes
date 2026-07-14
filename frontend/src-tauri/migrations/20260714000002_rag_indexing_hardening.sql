-- Composite indexes for archive repair/status queries and active-job
-- deduplication. Kept separate from the Phase 0 schema for existing installs.
CREATE INDEX IF NOT EXISTS idx_chunks_meeting_embedding_status
    ON chunks(meeting_id, embedding_status);

CREATE INDEX IF NOT EXISTS idx_jobs_kind_meeting_status
    ON jobs(kind, meeting_id, status);
