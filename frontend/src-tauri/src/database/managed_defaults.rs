//! One-time migration from historical bundled defaults to the managed pilot.
//!
//! Exact provider/model pairs are used deliberately: a non-default value is a
//! user choice and must survive an application update.

use serde::Serialize;
use sqlx::{Row, SqlitePool};

use crate::state::AppState;

const MARKER: &str = "migration.managed_pilot_defaults.v1";
const PENDING_CONFIRMATION: &str = "pending_confirmation";
const DEEPSEEK_MODEL: &str = crate::llm::providers::deepseek::DEFAULT_MODEL;
const SALUTESPEECH_MODEL: &str = "salutespeech-stream-v2";

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct MigrationReport {
    pub already_applied: bool,
    pub pending_confirmation: bool,
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
    let marker =
        sqlx::query_scalar::<_, String>("SELECT value FROM app_settings_kv WHERE key = ? LIMIT 1")
            .bind(MARKER)
            .fetch_optional(&mut *tx)
            .await?;

    if let Some(marker) = marker {
        tx.commit().await?;
        return Ok(MigrationReport {
            already_applied: marker != PENDING_CONFIRMATION,
            pending_confirmation: marker == PENDING_CONFIRMATION,
            ..MigrationReport::default()
        });
    }

    let mut report = MigrationReport::default();
    let local_only = sqlx::query_scalar::<_, String>(
        "SELECT value FROM app_settings_kv WHERE key = 'privacy.local_only' LIMIT 1",
    )
    .fetch_optional(&mut *tx)
    .await?
    .map(|value| value == "true" || value == "1")
    .unwrap_or(false);

    // Local-only is a reversible privacy preference, not a permanent decision
    // to skip the managed-default migration. Leave the marker unset so a later
    // startup can migrate the legacy defaults if the user disables local-only.
    if local_only {
        tx.commit().await?;
        return Ok(report);
    }

    let transcription_candidate = if let Some(row) =
        sqlx::query("SELECT provider, model FROM transcript_settings WHERE id = '1'")
            .fetch_optional(&mut *tx)
            .await?
    {
        let provider: String = row.try_get("provider")?;
        let model: String = row.try_get("model")?;
        is_legacy_transcription(&provider, &model)
    } else {
        false
    };
    let summary_candidate = if let Some(row) =
        sqlx::query("SELECT provider, model FROM settings WHERE id = '1'")
            .fetch_optional(&mut *tx)
            .await?
    {
        let provider: String = row.try_get("provider")?;
        let model: String = row.try_get("model")?;
        is_legacy_summary(&provider, &model)
    } else {
        false
    };

    // Updating the app must never switch local processing to cloud processing.
    // Stage the exact-default migration until the renderer obtains consent.
    report.pending_confirmation = transcription_candidate || summary_candidate;
    let marker_value = if report.pending_confirmation {
        PENDING_CONFIRMATION
    } else {
        "applied"
    };

    sqlx::query(
        "INSERT INTO app_settings_kv(key, value, updated_at) VALUES(?, ?, datetime('now')) \
         ON CONFLICT(key) DO NOTHING",
    )
    .bind(MARKER)
    .bind(marker_value)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(report)
}

pub async fn resolve(pool: &SqlitePool, accept: bool) -> Result<MigrationReport, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let marker =
        sqlx::query_scalar::<_, String>("SELECT value FROM app_settings_kv WHERE key = ? LIMIT 1")
            .bind(MARKER)
            .fetch_optional(&mut *tx)
            .await?;
    if marker.as_deref() != Some(PENDING_CONFIRMATION) {
        tx.commit().await?;
        return Ok(MigrationReport {
            already_applied: true,
            ..MigrationReport::default()
        });
    }

    if !accept {
        sqlx::query(
            "UPDATE app_settings_kv SET value='declined', updated_at=datetime('now') WHERE key=?",
        )
        .bind(MARKER)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(MigrationReport::default());
    }

    let local_only = sqlx::query_scalar::<_, String>(
        "SELECT value FROM app_settings_kv WHERE key = 'privacy.local_only' LIMIT 1",
    )
    .fetch_optional(&mut *tx)
    .await?
    .map(|value| value == "true" || value == "1")
    .unwrap_or(false);
    if local_only {
        sqlx::query(
            "UPDATE app_settings_kv SET value='declined_local_only', updated_at=datetime('now') WHERE key=?",
        )
        .bind(MARKER)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(MigrationReport::default());
    }

    let mut report = MigrationReport::default();
    let transcription = sqlx::query(
        "UPDATE transcript_settings SET provider='salutespeech', model=? WHERE id='1' \
         AND ((provider='gigaam' AND model='gigaam-v3-e2e-ctc') \
           OR (provider='parakeet' AND model='parakeet-tdt-0.6b-v3-int8'))",
    )
    .bind(SALUTESPEECH_MODEL)
    .execute(&mut *tx)
    .await?;
    report.transcription_changed = transcription.rows_affected() > 0;

    let summary = sqlx::query(
        "UPDATE settings SET provider='deepseek', model=? WHERE id='1' \
         AND ((provider='ollama' AND model='llama3.2:latest') \
           OR (provider='builtin-ai' AND model IN ('qwen3.5:2b','qwen3.5:4b')))",
    )
    .bind(DEEPSEEK_MODEL)
    .execute(&mut *tx)
    .await?;
    report.summary_changed = summary.rows_affected() > 0;

    sqlx::query(
        "UPDATE app_settings_kv SET value='applied', updated_at=datetime('now') WHERE key=?",
    )
    .bind(MARKER)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(report)
}

#[tauri::command]
pub async fn resolve_managed_defaults_migration(
    state: tauri::State<'_, AppState>,
    accept: bool,
) -> Result<MigrationReport, String> {
    resolve(state.db_manager.pool(), accept)
        .await
        .map_err(|error| error.to_string())
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
    async fn migrates_only_known_legacy_defaults_after_confirmation() {
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
                pending_confirmation: true,
                transcription_changed: false,
                summary_changed: false
            }
        );
        assert_eq!(
            values(&pool, "transcript_settings").await,
            ("gigaam".into(), "gigaam-v3-e2e-ctc".into())
        );
        assert_eq!(
            values(&pool, "settings").await,
            ("builtin-ai".into(), "qwen3.5:4b".into())
        );

        let report = resolve(&pool, true).await.unwrap();
        assert!(report.transcription_changed);
        assert!(report.summary_changed);
        assert_eq!(
            values(&pool, "transcript_settings").await,
            ("salutespeech".into(), SALUTESPEECH_MODEL.into())
        );
        assert_eq!(
            values(&pool, "settings").await,
            ("deepseek".into(), DEEPSEEK_MODEL.into())
        );
        assert!(migrate(&pool).await.unwrap().already_applied);
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

    #[tokio::test]
    async fn preserves_local_defaults_when_local_only_is_enabled() {
        let pool = pool().await;
        sqlx::query("INSERT INTO app_settings_kv(key,value) VALUES('privacy.local_only','true')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO transcript_settings VALUES('1','gigaam','gigaam-v3-e2e-ctc')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO settings VALUES('1','builtin-ai','qwen3.5:4b','large-v3')")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(migrate(&pool).await.unwrap(), MigrationReport::default());
        assert_eq!(
            values(&pool, "transcript_settings").await,
            ("gigaam".into(), "gigaam-v3-e2e-ctc".into())
        );
        assert_eq!(
            values(&pool, "settings").await,
            ("builtin-ai".into(), "qwen3.5:4b".into())
        );

        let marker: Option<String> =
            sqlx::query_scalar("SELECT value FROM app_settings_kv WHERE key = ?")
                .bind(MARKER)
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert_eq!(marker, None);

        sqlx::query("UPDATE app_settings_kv SET value='false' WHERE key='privacy.local_only'")
            .execute(&pool)
            .await
            .unwrap();
        let report = migrate(&pool).await.unwrap();
        assert!(report.pending_confirmation);
        assert_eq!(
            values(&pool, "settings").await,
            ("builtin-ai".into(), "qwen3.5:4b".into())
        );
        let report = resolve(&pool, true).await.unwrap();
        assert!(report.transcription_changed);
        assert!(report.summary_changed);
    }

    #[tokio::test]
    async fn declining_keeps_local_defaults_and_finishes_the_migration() {
        let pool = pool().await;
        sqlx::query("INSERT INTO transcript_settings VALUES('1','gigaam','gigaam-v3-e2e-ctc')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO settings VALUES('1','builtin-ai','qwen3.5:4b','large-v3')")
            .execute(&pool)
            .await
            .unwrap();

        assert!(migrate(&pool).await.unwrap().pending_confirmation);
        assert_eq!(
            resolve(&pool, false).await.unwrap(),
            MigrationReport::default()
        );
        assert_eq!(
            values(&pool, "transcript_settings").await,
            ("gigaam".into(), "gigaam-v3-e2e-ctc".into())
        );
        assert_eq!(
            values(&pool, "settings").await,
            ("builtin-ai".into(), "qwen3.5:4b".into())
        );
        assert!(migrate(&pool).await.unwrap().already_applied);
    }
}
