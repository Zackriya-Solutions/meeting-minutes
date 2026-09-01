ALTER TABLE translation_settings
ADD COLUMN translationLibreTranslateApiKey TEXT NOT NULL DEFAULT '';

ALTER TABLE settings
ADD COLUMN translationLibreTranslateApiKey TEXT NOT NULL DEFAULT '';
