CREATE TABLE IF NOT EXISTS translation_settings (
    id TEXT PRIMARY KEY,
    translationEnabled INTEGER NOT NULL DEFAULT 0,
    translationEngine TEXT NOT NULL DEFAULT 'ollama',
    translationTargetLanguage TEXT NOT NULL DEFAULT 'pt-BR',
    translationLibreTranslateEndpoint TEXT NOT NULL DEFAULT '',
    translationOllamaEndpoint TEXT NOT NULL DEFAULT 'http://localhost:11434',
    translationOllamaModel TEXT NOT NULL DEFAULT 'aya-expanse:latest'
);

-- Preserve the pre-release translation settings once, while leaving the
-- summary provider's ollamaEndpoint column owned by summary settings.
INSERT OR IGNORE INTO translation_settings (
    id, translationEnabled, translationEngine, translationTargetLanguage,
    translationLibreTranslateEndpoint, translationOllamaEndpoint, translationOllamaModel
)
SELECT
    id, translationEnabled, translationEngine, translationTargetLanguage,
    translationLibreTranslateEndpoint,
    COALESCE(NULLIF(ollamaEndpoint, ''), 'http://localhost:11434'),
    translationOllamaModel
FROM settings
WHERE id = '1';
