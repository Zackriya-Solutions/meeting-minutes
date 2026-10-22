// audio/transcription/gemini_live/settings.rs
//
// Gemini Live settings, stored in the `gemini_live_settings` table.

use super::protocol::SetupOptions;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

pub const MAX_VOCABULARY_TERMS: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiLiveSettings {
    /// "roman" | "devanagari" | "auto"
    pub language_mode: String,
    pub custom_vocabulary: Vec<String>,
    /// "VERBATIM" | "SMART"
    pub transcription_mode: String,
    /// "parakeet" | "localWhisper"
    pub fallback_provider: String,
}

impl Default for GeminiLiveSettings {
    fn default() -> Self {
        Self {
            language_mode: "roman".into(),
            custom_vocabulary: Vec::new(),
            transcription_mode: "VERBATIM".into(),
            fallback_provider: "parakeet".into(),
        }
    }
}

impl GeminiLiveSettings {
    /// Clamp unknown values to defaults and clean the vocabulary list.
    pub fn normalized(mut self) -> Self {
        if !matches!(self.language_mode.as_str(), "roman" | "devanagari" | "auto") {
            self.language_mode = "roman".into();
        }
        if !matches!(self.transcription_mode.as_str(), "VERBATIM" | "SMART") {
            self.transcription_mode = "VERBATIM".into();
        }
        if !matches!(self.fallback_provider.as_str(), "parakeet" | "localWhisper") {
            self.fallback_provider = "parakeet".into();
        }
        let mut seen = std::collections::HashSet::new();
        self.custom_vocabulary = self
            .custom_vocabulary
            .iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty() && seen.insert(t.to_lowercase()))
            .take(MAX_VOCABULARY_TERMS)
            .collect();
        self
    }

    /// Language codes were chosen from live tests with Hindi-English speech:
    /// en-IN returns consistent romanized Hinglish, hi-IN consistent Devanagari (English
    /// terms stay in Latin), and auto-detect mixes the two scripts between segments.
    pub fn setup_options(&self) -> SetupOptions {
        let language_codes = match self.language_mode.as_str() {
            "devanagari" => vec!["hi-IN".to_string()],
            "auto" => Vec::new(),
            _ => vec!["en-IN".to_string()],
        };
        SetupOptions {
            language_codes,
            custom_vocabulary: self.custom_vocabulary.clone(),
            mode: Some(self.transcription_mode.clone()),
        }
    }
}

pub async fn load(pool: &SqlitePool) -> Result<(GeminiLiveSettings, Option<String>), sqlx::Error> {
    let row: Option<(Option<String>, String, String, String, String)> = sqlx::query_as(
        "SELECT api_key, language_mode, custom_vocabulary, transcription_mode, fallback_provider \
         FROM gemini_live_settings WHERE id = '1'",
    )
    .fetch_optional(pool)
    .await?;

    Ok(match row {
        Some((api_key, language_mode, vocabulary, transcription_mode, fallback_provider)) => (
            GeminiLiveSettings {
                language_mode,
                custom_vocabulary: serde_json::from_str(&vocabulary).unwrap_or_default(),
                transcription_mode,
                fallback_provider,
            }
            .normalized(),
            api_key.filter(|k| !k.trim().is_empty()),
        ),
        None => (GeminiLiveSettings::default(), None),
    })
}

/// Save settings. `api_key`: Some(non-empty) replaces the key, Some("") clears it,
/// None keeps the stored key.
pub async fn save(
    pool: &SqlitePool,
    settings: &GeminiLiveSettings,
    api_key: Option<&str>,
) -> Result<(), sqlx::Error> {
    let vocabulary = serde_json::to_string(&settings.custom_vocabulary).unwrap_or_else(|_| "[]".into());
    sqlx::query(
        "INSERT INTO gemini_live_settings (id, language_mode, custom_vocabulary, transcription_mode, fallback_provider) \
         VALUES ('1', $1, $2, $3, $4) \
         ON CONFLICT(id) DO UPDATE SET language_mode = excluded.language_mode, \
         custom_vocabulary = excluded.custom_vocabulary, transcription_mode = excluded.transcription_mode, \
         fallback_provider = excluded.fallback_provider",
    )
    .bind(&settings.language_mode)
    .bind(&vocabulary)
    .bind(&settings.transcription_mode)
    .bind(&settings.fallback_provider)
    .execute(pool)
    .await?;

    if let Some(key) = api_key {
        let key = key.trim();
        sqlx::query("UPDATE gemini_live_settings SET api_key = $1 WHERE id = '1'")
            .bind(if key.is_empty() { None } else { Some(key) })
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// The Gemini key saved for summaries (Settings → Summary), used when no
/// transcription-specific key is set.
pub async fn summary_gemini_key(pool: &SqlitePool) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>("SELECT geminiApiKey FROM settings WHERE id = '1' LIMIT 1")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten()
        .filter(|k| !k.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_modes_map_to_codes() {
        let mut s = GeminiLiveSettings::default();
        assert_eq!(s.setup_options().language_codes, vec!["en-IN"]);
        s.language_mode = "devanagari".into();
        assert_eq!(s.setup_options().language_codes, vec!["hi-IN"]);
        s.language_mode = "auto".into();
        assert!(s.setup_options().language_codes.is_empty());
    }

    #[test]
    fn normalizes_values_and_vocabulary() {
        let s = GeminiLiveSettings {
            language_mode: "klingon".into(),
            custom_vocabulary: vec![" Meetily ".into(), "".into(), "meetily".into(), "BigQuery".into()],
            transcription_mode: "loud".into(),
            fallback_provider: "deepgram".into(),
        }
        .normalized();
        assert_eq!(s.language_mode, "roman");
        assert_eq!(s.transcription_mode, "VERBATIM");
        assert_eq!(s.fallback_provider, "parakeet");
        assert_eq!(s.custom_vocabulary, vec!["Meetily", "BigQuery"]);
    }

    #[tokio::test]
    async fn save_and_load_through_real_migrations() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        assert_eq!(load(&pool).await.unwrap(), (GeminiLiveSettings::default(), None));

        let s = GeminiLiveSettings {
            language_mode: "devanagari".into(),
            custom_vocabulary: vec!["Meetily".into()],
            transcription_mode: "SMART".into(),
            fallback_provider: "localWhisper".into(),
        };
        save(&pool, &s, Some("key-1234")).await.unwrap();
        assert_eq!(load(&pool).await.unwrap(), (s.clone(), Some("key-1234".into())));

        save(&pool, &s, None).await.unwrap(); // None keeps the key
        assert_eq!(load(&pool).await.unwrap().1, Some("key-1234".into()));
        save(&pool, &s, Some("")).await.unwrap(); // "" removes it
        assert_eq!(load(&pool).await.unwrap().1, None);

        // Falls back to the summary Gemini key.
        assert_eq!(summary_gemini_key(&pool).await, None);
        sqlx::query("INSERT INTO settings (id, provider, model, whisperModel, geminiApiKey) VALUES ('1', 'ollama', 'm', 'w', 'summary-key')")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(summary_gemini_key(&pool).await, Some("summary-key".into()));
    }

    #[test]
    fn caps_vocabulary_size() {
        let s = GeminiLiveSettings {
            custom_vocabulary: (0..1500).map(|i| format!("term{}", i)).collect(),
            ..Default::default()
        }
        .normalized();
        assert_eq!(s.custom_vocabulary.len(), MAX_VOCABULARY_TERMS);
    }
}
