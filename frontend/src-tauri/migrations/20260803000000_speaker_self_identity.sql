-- The audio channel only tells us where sound came from. It does not identify
-- the person speaking, especially during in-person meetings recorded by one mic.
-- Persist the user's identity on the diarized voice profile instead.
ALTER TABLE speakers
ADD COLUMN is_self INTEGER NOT NULL DEFAULT 0 CHECK (is_self IN (0, 1));

-- A local Memento profile has at most one owner voice. The row can be cleared
-- when the user removes the assignment and reassigned transactionally later.
CREATE UNIQUE INDEX IF NOT EXISTS idx_speakers_single_self
ON speakers(is_self)
WHERE is_self = 1;
