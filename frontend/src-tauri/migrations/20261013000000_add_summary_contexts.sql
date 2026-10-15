-- Summary context kept at three levels: global, per folder, per meeting.
CREATE TABLE IF NOT EXISTS summary_contexts (
    scope         TEXT NOT NULL CHECK (scope IN ('global', 'folder', 'meeting')),
    scope_id      TEXT NOT NULL DEFAULT '',
    context       TEXT NOT NULL DEFAULT '',
    use_inherited INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    PRIMARY KEY (scope, scope_id)
);

-- A meeting's context goes away with the meeting, whichever code path deletes it.
CREATE TRIGGER IF NOT EXISTS trg_summary_contexts_meeting_deleted
AFTER DELETE ON meetings
BEGIN
    DELETE FROM summary_contexts WHERE scope = 'meeting' AND scope_id = OLD.id;
END;
