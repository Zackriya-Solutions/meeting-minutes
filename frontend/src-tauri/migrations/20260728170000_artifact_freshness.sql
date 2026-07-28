ALTER TABLE meeting_outcomes
    ADD COLUMN stale INTEGER NOT NULL DEFAULT 0;

ALTER TABLE meeting_outcomes
    ADD COLUMN source_transcript_revision INTEGER NOT NULL DEFAULT 0;

ALTER TABLE analytics_reports
    ADD COLUMN stale INTEGER NOT NULL DEFAULT 0;

ALTER TABLE summary_processes
    ADD COLUMN stale INTEGER NOT NULL DEFAULT 0;
