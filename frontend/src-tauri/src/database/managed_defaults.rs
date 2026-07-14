//! One-time migration from historical bundled defaults to the managed pilot.
//!
//! Exact provider/model pairs are used deliberately: a non-default value is a
//! user choice and must survive an application update.

use sqlx::{Row, SqlitePool};

const MARKER: &str = "migration.managed_pilot_defaults.v1";
const DEEPSEEK_MODEL: &str = crate::llm::providers::deepseek::DEFAULT_MODEL;
const SALUTESPEECH_MODEL: &str = "salutespeech-stream-v2";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct MigrationReport {
    pub already_applied: bool,
    pub transcription_changed: bool,
    pub summary_changed: bool,
}

fn is_legacy_transcription(provider: &str, model: &str) -> bool {
    matches!(
        (provider, model),
        ("gigaam", "gigaam-v3-e2e-ctc") | ("parakeet", "parakeet-tdt-0.6b-v3-int8")
    )
}

fn is_legacy_summary(provider: &str, model: &str) -> bool {
    matches!(
        (provider, model),
        ("ollama", "llama3.2:latest") | ("builtin-ai", "qwen3.5:2b") | ("builtin-ai", "qwen3.5:4b")
    )
}

pub async fn migrate(pool: &SqlitePool) -> Result<MigrationReport, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let applied =
        sqlx::query_scalar::<_, String>("SELECT value FROM app_settings_kv WHERE key = ? LIMIT 1")
            .bind(MARKER)
            .fetch_optional(&mut *tx)
            .await?
            .is_some();

    if applied {
        tx.commit().await?;
        return Ok(MigrationReport {
            already_applied: true,
            ..MigrationReport::default()
        });
    }

    let mut report = MigrationReport::default();

    if let Some(row) = sqlx::query("SELECT provider, model FROM transcript_settings WHERE id = '1'")
        .fetch_optional(&mut *tx)
        .await?
    {
        let provider: String = row.try_get("provider")?;
        let model: String = row.try_get("model")?;
        if is_legacy_transcription(&provider, &model) {
            sqlx::query("UPDATE transcript_settings SET provider = 'salutespeech', model = ? WHERE id = '1'")
                .bind(SALUTESPEECH_MODEL)
                .execute(&mut *tx)
                .await?;
            report.transcription_changed = true;
        }
    }

    if let Some(row) = sqlx::query("SELECT provider, model FROM settings WHERE id = '1'")
        .fetch_optional(&mut *tx)
        .await?
    {
        let provider: String = row.try_get("provider")?;
        let model: String = row.try_get("model")?;
        if is_legacy_summary(&provider, &model) {
            sqlx::query("UPDATE settings SET provider = 'deepseek', model = ? WHERE id = '1'")
                .bind(DEEPSEEK_MODEL)
                .execute(&mut *tx)
                .await?;
            report.summary_changed = true;
        }
    }

    sqlx::query(
        "INSERT INTO app_settings_kv(key, value, updated_at) VALUES(?, 'applied', datetime('now'))",
    )
    .bind(MARKER)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE app_settings_kv(key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL DEFAULT (datetime('now')))")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE transcript_settings(id TEXT PRIMARY KEY, provider TEXT NOT NULL, model TEXT NOT NULL)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE settings(id TEXT PRIMARY KEY, provider TEXT NOT NULL, model TEXT NOT NULL, whisperModel TEXT NOT NULL)")
            .execute(&pool).await.unwrap();
        pool
    }

    async fn values(pool: &SqlitePool, table: &str) -> (String, String) {
        let sql = format!("SELECT provider, model FROM {table} WHERE id = '1'");
        let row = sqlx::query(&sql).fetch_one(pool).await.unwrap();
        (row.get("provider"), row.get("model"))
    }

    #[tokio::test]
    async fn migrates_only_known_legacy_defaults() {
        let pool = pool().await;
        sqlx::query("INSERT INTO transcript_settings VALUES('1','gigaam','gigaam-v3-e2e-ctc')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO settings VALUES('1','builtin-ai','qwen3.5:4b','large-v3')")
            .execute(&pool)
            .await
            .unwrap();

        let report = migrate(&pool).await.unwrap();
        assert_eq!(
            report,
            MigrationReport {
                already_applied: false,
                transcription_changed: true,
                summary_changed: true
            }
        );
        assert_eq!(
            values(&pool, "transcript_settings").await,
            ("salutespeech".into(), SALUTESPEECH_MODEL.into())
        );
        assert_eq!(
            values(&pool, "settings").await,
            ("deepseek".into(), DEEPSEEK_MODEL.into())
        );
    }

    #[tokio::test]
    async fn preserves_explicit_user_choices_and_runs_once() {
        let pool = pool().await;
        sqlx::query("INSERT INTO transcript_settings VALUES('1','openai','whisper-1')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO settings VALUES('1','openrouter','custom-model','large-v3')")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(migrate(&pool).await.unwrap(), MigrationReport::default());
        assert_eq!(
            values(&pool, "transcript_settings").await,
            ("openai".into(), "whisper-1".into())
        );
        assert_eq!(
            values(&pool, "settings").await,
            ("openrouter".into(), "custom-model".into())
        );

        sqlx::query("UPDATE transcript_settings SET provider='gigaam', model='gigaam-v3-e2e-ctc' WHERE id='1'")
            .execute(&pool).await.unwrap();
        assert!(migrate(&pool).await.unwrap().already_applied);
        assert_eq!(
            values(&pool, "transcript_settings").await,
            ("gigaam".into(), "gigaam-v3-e2e-ctc".into())
        );
    }
}
