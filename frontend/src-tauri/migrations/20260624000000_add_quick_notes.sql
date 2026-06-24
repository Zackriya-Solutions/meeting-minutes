-- Quick Note: lightweight per-day checklist cards with end-of-day rollover.
-- `carried_from` records the original date when a pending card is carried
-- forward; `archived` guards against re-archiving an already-rolled-over day.
CREATE TABLE IF NOT EXISTS quick_notes (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    date          TEXT    NOT NULL,           -- YYYY-MM-DD (local)
    text          TEXT    NOT NULL DEFAULT '',
    done          INTEGER NOT NULL DEFAULT 0, -- 0 = ❌ pending, 1 = ✅ done
    created_at    TEXT    NOT NULL,
    carried_from  TEXT,                       -- original date if carried forward
    archived      INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_quick_notes_date ON quick_notes(date);
CREATE INDEX IF NOT EXISTS idx_quick_notes_archived ON quick_notes(archived);
