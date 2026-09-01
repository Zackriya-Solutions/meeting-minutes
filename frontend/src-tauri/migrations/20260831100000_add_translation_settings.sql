ALTER TABLE settings ADD COLUMN translationEnabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE settings ADD COLUMN translationEngine TEXT NOT NULL DEFAULT 'ollama';
ALTER TABLE settings ADD COLUMN translationTargetLanguage TEXT NOT NULL DEFAULT 'pt-BR';
ALTER TABLE settings ADD COLUMN translationLibreTranslateEndpoint TEXT NOT NULL DEFAULT '';
ALTER TABLE settings ADD COLUMN translationOllamaModel TEXT NOT NULL DEFAULT 'aya-expanse:latest';
