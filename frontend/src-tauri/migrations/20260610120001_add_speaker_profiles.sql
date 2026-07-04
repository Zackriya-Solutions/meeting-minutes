-- Persistent voice profiles for speaker identification.
-- One centroid embedding per named profile (f32 little-endian BLOB).
-- Embeddings are derived locally and never leave the device.
CREATE TABLE IF NOT EXISTS speaker_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    embedding BLOB NOT NULL,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);
