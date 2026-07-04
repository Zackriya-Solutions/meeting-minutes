-- Speaker identification (diarization) feature settings.
-- Single-row table; 'enabled' gates the live diarization pipeline.
CREATE TABLE IF NOT EXISTS diarization_settings (
    id TEXT PRIMARY KEY DEFAULT '1',
    enabled INTEGER NOT NULL DEFAULT 0
);
