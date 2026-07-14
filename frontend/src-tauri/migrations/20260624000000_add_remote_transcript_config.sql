-- Migration: Add remoteConfig to transcript_settings table
-- Supports the new "remote" transcription provider (HTTPS-backed, vendor-neutral).

ALTER TABLE transcript_settings ADD COLUMN remoteConfig TEXT;
