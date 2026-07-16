-- Applied migrations are immutable. Repair timestamps accepted by SQLite's permissive date()
-- handling, then replace the trigger with a strict Julian-day round-trip check.

UPDATE meetings
SET occurred_at = NULL
WHERE substr(title, 11, 1) = '_'
  AND substr(title, 17, 1) = '_'
  AND occurred_at = substr(title, 1, 10) || 'T' ||
                    replace(substr(title, 12, 5), '-', ':') || ':00'
  AND COALESCE(
        strftime(
            '%Y-%m-%dT%H:%M',
            julianday(
                substr(title, 1, 10) || 'T' ||
                replace(substr(title, 12, 5), '-', ':') || ':00'
            )
        ),
        ''
      ) != (
        substr(title, 1, 10) || 'T' ||
        replace(substr(title, 12, 5), '-', ':')
      );

DROP TRIGGER IF EXISTS set_imported_meeting_occurred_at;

CREATE TRIGGER set_imported_meeting_occurred_at
AFTER INSERT ON meetings
WHEN NEW.occurred_at IS NULL
 AND substr(NEW.title, 11, 1) = '_'
 AND substr(NEW.title, 17, 1) = '_'
 AND strftime(
        '%Y-%m-%dT%H:%M',
        julianday(
            substr(NEW.title, 1, 10) || 'T' ||
            replace(substr(NEW.title, 12, 5), '-', ':') || ':00'
        )
     ) = (
        substr(NEW.title, 1, 10) || 'T' ||
        replace(substr(NEW.title, 12, 5), '-', ':')
     )
BEGIN
    UPDATE meetings
    SET occurred_at = substr(NEW.title, 1, 10) || 'T' ||
                      replace(substr(NEW.title, 12, 5), '-', ':') || ':00'
    WHERE id = NEW.id;
END;
