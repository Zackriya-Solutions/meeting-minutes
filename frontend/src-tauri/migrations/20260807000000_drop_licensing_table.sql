-- Migration: Drop the unused PRO licensing table
--
-- 20251105120000_add_pro_license_custom_openai.sql created this table for a PRO
-- license check that this fork does not have. Nothing ever read it: no repository
-- queries it and no Tauri command exposes it.
--
-- The two earlier licensing migrations are deliberately left in place. sqlx
-- validates the checksums of already-applied migrations, so deleting them would
-- break startup for every existing install -- and 20251105120000 also adds
-- settings.customOpenAIConfig, which is a live feature.

DROP TABLE IF EXISTS licensing;
