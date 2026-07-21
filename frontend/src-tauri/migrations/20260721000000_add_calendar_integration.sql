-- Migration: Add Google Calendar integration support

-- Calendar-derived metadata on a meeting, written after a match against a Google Calendar event
ALTER TABLE meetings ADD COLUMN calendar_event_id TEXT;
ALTER TABLE meetings ADD COLUMN calendar_attendees TEXT; -- JSON array of {name, email}
ALTER TABLE meetings ADD COLUMN calendar_meet_link TEXT;
ALTER TABLE meetings ADD COLUMN calendar_start_time TEXT;
ALTER TABLE meetings ADD COLUMN calendar_end_time TEXT;

-- Google OAuth token bundle: {client_id, client_secret, access_token, refresh_token, token_expiry, scope}
ALTER TABLE settings ADD COLUMN googleCalendarConfig TEXT;

-- Phase B toggle: auto-start/stop recording on detected Google Meet calls
ALTER TABLE settings ADD COLUMN autoDetectMeetEnabled INTEGER DEFAULT 0;
