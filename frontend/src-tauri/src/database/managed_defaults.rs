//! One-time migration from historical bundled defaults to the managed pilot.
//!
//! Exact provider/model pairs are used deliberately: a non-default value is a
//! user choice and must survive an application update.

use serde::Serialize;
use sqlx::{Row, SqlitePool};

use crate::state::AppState;

const MARKER: &str = "migration.managed_pilot_defaults.v1";
const PENDING_TRANSCRIPTION: &str = "pending_confirmation:transcription";
const PENDING_SUMMARY: &str = "pending_confirmation:summary";
const PENDING_BOTH: &str = "pending_confirmation:transcription,summary";
const DEEPSEEK_MODEL: &str = crate::llm::providers::deepseek::DEFAULT_MODEL;
const SALUTESPEECH_MODEL: &str = "salutespeech-stream-v2";

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct MigrationReport {
    pub already_applied: bool,
    pub pending_confirmation: bool,
    pub transcription_changed: bool,
    pub summary_changed: bool,
}

/// Transcription is never migrated to the cloud anymore. Measured 2026-07-20 on a real
/// 31-min meeting: SaluteSpeech matched 80.4% of reference words (395 dropped) vs
/// GigaAM's 92.4% on identical segmentation, and its diarization found 4 of 7 speakers
/// (68.8% agreement) vs the local engine's 7/7 (92.5%). Local models are the default;
/// installs previously migrated to salutespeech are moved back by the
/// `default_local_models` DB migration.
fn is_legacy_transcription(_provider: &str, _model: &str) -> bool {
    false
}

fn is_legacy_summary(provider: &str, model: &str) -> bool {
    matches!(
        (provider, model),
        ("ollama", "llama3.2:latest") | ("builtin-ai", "qwen3.5:2b") | ("builtin-ai", "qwen3.5:4b")
    )
}

fn pending_candidates(marker: &str) -> Option<(bool, bool)> {
    match marker {
        PENDING_TRANSCRIPTION => Some((true, false)),
        PENDING_SUMMARY => Some((false, true)),
        PENDING_BOTH => Some((true, true)),
        _ => None,
    }
}

async fn configured_value(pool: &SqlitePool, key: &str, envs: &[&str]) -> bool {
    let configured =
        sqlx::query_scalar::<_, String>("SELECT value FROM app_settings_kv WHERE key = ? LIMIT 1")
            .bind(key)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .is_some_and(|value| !value.trim().is_empty());
    configured
        || envs
            .iter()
            .any(|name| std::env::var(name).is_ok_and(|value| !value.trim().is_empty()))
}

async fn direct_transcription_credentials(pool: &SqlitePool) -> bool {
    configured_value(
        pool,
        "salutespeech.auth_key",
        &["SALUTESPEECH_AUTH_KEY", "SBER_SALUTE_AUTH_KEY"],
    )
    .await
}

async fn direct_summary_credentials(pool: &SqlitePool) -> bool {
    configured_value(pool, "deepseek.api_key", &["DEEPSEEK_API_KEY"]).await
}

async fn managed_capabilities(pool: &SqlitePool) -> (bool, bool) {
    let gateway = crate::gateway_identity::managed_gateway_supported();
    (
        gateway || direct_transcription_credentials(pool).await,
        gateway || direct_summary_credentials(pool).await,
    )
}

async fn managed_targets_available(
    pool: &SqlitePool,
    transcription: bool,
    summary: bool,
) -> (bool, bool) {
    let direct_transcription = !transcription || direct_transcription_credentials(pool).await;
    let direct_summary = !summary || direct_summary_credentials(pool).await;
    if direct_transcription && direct_summary {
        return (true, true);
    }
    let gateway = crate::gateway_identity::install_token().await.is_ok();
    (
        !transcription || direct_transcription || gateway,
        !summary || direct_summary || gateway,
    )
}

pub async fn migrate(pool: &SqlitePool) -> Result<MigrationReport, sqlx::Error> {
    let (transcription_capable, summary_capable) = managed_capabilities(pool).await;
    let mut tx = pool.begin().await?;
    let marker =
        sqlx::query_scalar::<_, String>("SELECT value FROM app_settings_kv WHERE key = ? LIMIT 1")
            .bind(MARKER)
            .fetch_optional(&mut *tx)
            .await?;

    if let Some(marker) = marker {
        let pending = pending_candidates(&marker);
        tx.commit().await?;
        return Ok(MigrationReport {
            already_applied: pending.is_none(),
            pending_confirmation: pending.is_some(),
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

    let legacy_transcription = if let Some(row) =
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
    let legacy_summary = if let Some(row) =
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

    // Capability can be temporarily absent in a development or unsigned build.
    // Do not turn that transient state into the same terminal marker used after
    // explicit user confirmation; the official build should still offer consent.
    if (legacy_transcription && !transcription_capable) || (legacy_summary && !summary_capable) {
        tx.commit().await?;
        return Ok(report);
    }

    let transcription_candidate = transcription_capable && legacy_transcription;
    let summary_candidate = summary_capable && legacy_summary;

    // Updating the app must never switch local processing to cloud processing.
    // Stage the exact-default migration until the renderer obtains consent.
    report.pending_confirmation = transcription_candidate || summary_candidate;
    let marker_value = match (transcription_candidate, summary_candidate) {
        (true, true) => PENDING_BOTH,
        (true, false) => PENDING_TRANSCRIPTION,
        (false, true) => PENDING_SUMMARY,
        (false, false) => "applied",
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
    let marker =
        sqlx::query_scalar::<_, String>("SELECT value FROM app_settings_kv WHERE key = ? LIMIT 1")
            .bind(MARKER)
            .fetch_optional(pool)
            .await?;
    let Some((_pending_transcription, pending_summary)) =
        marker.as_deref().and_then(pending_candidates)
    else {
        return Ok(MigrationReport {
            already_applied: true,
            ..MigrationReport::default()
        });
    };

    if accept {
        // Transcription no longer migrates to the cloud (see is_legacy_transcription);
        // only the summary target's availability can block acceptance of a stale marker.
        let (_, summary_available) =
            managed_targets_available(pool, false, pending_summary).await;
        if !summary_available {
            return Err(sqlx::Error::Protocol(
                "managed providers are unavailable; local providers remain unchanged".to_string(),
            ));
        }
    }

    let mut tx = pool.begin().await?;
    let marker =
        sqlx::query_scalar::<_, String>("SELECT value FROM app_settings_kv WHERE key = ? LIMIT 1")
            .bind(MARKER)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((pending_transcription, pending_summary)) =
        marker.as_deref().and_then(pending_candidates)
    else {
        tx.commit().await?;
        return Ok(MigrationReport {
            already_applied: true,
            ..MigrationReport::default()
        });
    };

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
    // A stale PENDING_TRANSCRIPTION/PENDING_BOTH marker from an older build resolves
    // with the transcription config untouched — transcription stays on local models.
    let _ = pending_transcription;

    if pending_summary {
        if let Some(row) = sqlx::query("SELECT provider, model FROM settings WHERE id = '1'")
            .fetch_optional(&mut *tx)
            .await?
        {
            let provider: String = row.try_get("provider")?;
            let model: String = row.try_get("model")?;
            if is_legacy_summary(&provider, &model) {
                let updated = sqlx::query(
                    "UPDATE settings SET provider='deepseek', model=? \
                     WHERE id='1' AND provider=? AND model=?",
                )
                .bind(DEEPSEEK_MODEL)
                .bind(provider)
                .bind(model)
                .execute(&mut *tx)
                .await?;
                report.summary_changed = updated.rows_affected() > 0;
            }
        }
    }

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

    async fn enable_managed_providers(pool: &SqlitePool) {
        sqlx::query(
            "INSERT INTO app_settings_kv(key,value) VALUES \
             ('salutespeech.auth_key','test-salute-key'), \
             ('deepseek.api_key','test-deepseek-key')",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn fresh_database_never_prompts_for_legacy_migration() {
        let pool = pool().await;
        enable_managed_providers(&pool).await;

        let report = migrate(&pool).await.unwrap();
        assert_eq!(
            report,
            MigrationReport {
                already_applied: false,
                pending_confirmation: false,
                transcription_changed: false,
                summary_changed: false,
            }
        );

        let marker: String =
            sqlx::query_scalar("SELECT value FROM app_settings_kv WHERE key = ?")
                .bind(MARKER)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(marker, "applied");
    }

    #[tokio::test]
    async fn migrates_only_known_legacy_defaults_after_confirmation() {
        let pool = pool().await;
        enable_managed_providers(&pool).await;
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
        assert!(
            !report.transcription_changed,
            "transcription never migrates to the cloud"
        );
        assert!(report.summary_changed);
        assert_eq!(
            values(&pool, "transcript_settings").await,
            ("gigaam".into(), "gigaam-v3-e2e-ctc".into()),
            "transcription config stays local"
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
        enable_managed_providers(&pool).await;
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
        assert!(
            !report.transcription_changed,
            "transcription never migrates to the cloud"
        );
        assert!(report.summary_changed);
    }

    #[tokio::test]
    async fn declining_keeps_local_defaults_and_finishes_the_migration() {
        let pool = pool().await;
        enable_managed_providers(&pool).await;
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

    #[tokio::test]
    async fn transcription_is_never_a_cloud_migration_candidate() {
        // Legacy transcription config + non-legacy summary: nothing to migrate, no
        // prompt — transcription stays on local models by policy.
        let pool = pool().await;
        enable_managed_providers(&pool).await;
        sqlx::query("INSERT INTO transcript_settings VALUES('1','gigaam','gigaam-v3-e2e-ctc')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO settings VALUES('1','openrouter','custom-model','large-v3')")
            .execute(&pool)
            .await
            .unwrap();

        let report = migrate(&pool).await.unwrap();
        assert!(!report.pending_confirmation);
        let marker: String = sqlx::query_scalar("SELECT value FROM app_settings_kv WHERE key = ?")
            .bind(MARKER)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(marker, "applied");
        assert_eq!(
            values(&pool, "transcript_settings").await,
            ("gigaam".into(), "gigaam-v3-e2e-ctc".into())
        );

        // A stale pending-transcription marker from an older build resolves cleanly
        // without touching the transcription config.
        sqlx::query("UPDATE app_settings_kv SET value=? WHERE key=?")
            .bind(PENDING_TRANSCRIPTION)
            .bind(MARKER)
            .execute(&pool)
            .await
            .unwrap();
        let report = resolve(&pool, true).await.unwrap();
        assert!(!report.transcription_changed);
        assert!(!report.summary_changed);
        assert_eq!(
            values(&pool, "transcript_settings").await,
            ("gigaam".into(), "gigaam-v3-e2e-ctc".into())
        );
    }

    #[tokio::test]
    async fn unavailable_build_keeps_working_local_defaults_without_a_prompt() {
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
        assert!(!report.pending_confirmation);
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

        enable_managed_providers(&pool).await;
        assert!(migrate(&pool).await.unwrap().pending_confirmation);
    }

    #[tokio::test]
    async fn unavailable_build_preserves_already_selected_managed_providers() {
        let pool = pool().await;
        sqlx::query("INSERT INTO app_settings_kv(key,value) VALUES(?, 'applied')")
            .bind(MARKER)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO transcript_settings VALUES('1','salutespeech',?1)")
            .bind(SALUTESPEECH_MODEL)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO settings VALUES('1','deepseek',?1,'large-v3')")
            .bind(DEEPSEEK_MODEL)
            .execute(&pool)
            .await
            .unwrap();

        let report = migrate(&pool).await.unwrap();
        assert!(report.already_applied);
        assert!(!report.transcription_changed);
        assert!(!report.summary_changed);
        assert_eq!(
            values(&pool, "transcript_settings").await,
            ("salutespeech".into(), SALUTESPEECH_MODEL.into())
        );
        assert_eq!(
            values(&pool, "settings").await,
            ("deepseek".into(), DEEPSEEK_MODEL.into())
        );
        let marker: String = sqlx::query_scalar("SELECT value FROM app_settings_kv WHERE key = ?")
            .bind(MARKER)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(marker, "applied");
    }
}
