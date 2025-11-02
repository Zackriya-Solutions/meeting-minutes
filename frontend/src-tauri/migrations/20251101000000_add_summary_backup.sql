-- Migration: Add backup columns for summary regeneration
-- This allows preserving the previous summary when regeneration fails or is cancelled

ALTER TABLE summary_processes
ADD COLUMN result_backup TEXT;

ALTER TABLE summary_processes
ADD COLUMN result_backup_timestamp TEXT;
