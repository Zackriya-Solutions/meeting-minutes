-- Keep the two already-applied standup migrations immutable. The first repair required the
-- date/time separator at byte 11; this follow-up also requires the separator after HH-MM.

UPDATE meetings
SET occurred_at = NULL
WHERE substr(title, 11, 1) = '_'
  AND substr(title, 17, 1) != '_'
  AND date(substr(title, 1, 10)) IS NOT NULL
  AND time(replace(substr(title, 12, 5), '-', ':')) IS NOT NULL
  AND occurred_at = substr(title, 1, 10) || 'T' ||
                    replace(substr(title, 12, 5), '-', ':') || ':00';

DROP TRIGGER IF EXISTS set_imported_meeting_occurred_at;

CREATE TRIGGER set_imported_meeting_occurred_at
AFTER INSERT ON meetings
WHEN NEW.occurred_at IS NULL
 AND date(substr(NEW.title, 1, 10)) IS NOT NULL
 AND substr(NEW.title, 11, 1) = '_'
 AND substr(NEW.title, 17, 1) = '_'
 AND time(replace(substr(NEW.title, 12, 5), '-', ':')) IS NOT NULL
BEGIN
    UPDATE meetings
    SET occurred_at = substr(NEW.title, 1, 10) || 'T' ||
                      replace(substr(NEW.title, 12, 5), '-', ':') || ':00'
    WHERE id = NEW.id;
END;
