ALTER TABLE calendar_accounts ADD COLUMN auto_start_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE calendar_accounts ADD COLUMN auto_start_mode TEXT NOT NULL DEFAULT 'ask'; -- 'ask' | 'silent'
ALTER TABLE calendar_accounts ADD COLUMN auto_stop_grace_minutes INTEGER NOT NULL DEFAULT 5;
