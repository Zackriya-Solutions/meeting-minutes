-- Add language field to settings table
-- Date: 2025-11-13
-- Author: Luiz
-- Description: Adds language field to support internationalization (i18n)
--              Default language is 'pt' (Portuguese - Brazil)
--              Supported languages: 'pt' (Portuguese), 'en' (English)

ALTER TABLE settings ADD COLUMN language TEXT NOT NULL DEFAULT 'pt';
