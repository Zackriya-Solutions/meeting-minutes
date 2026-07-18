//! Unified LLM access layer (PLAN.md §9 cross-cutting convention).
//!
//! Every LLM call carries a [`Purpose`] and passes through [`guarded_complete`], which
//! enforces the privacy toggles CENTRALLY (not just in the UI) — the guard runs BEFORE
//! the provider future is awaited, so a disabled purpose or local-only mode produces
//! ZERO outbound network calls (PLAN.md §8 acceptance criterion).
//!
//! Actual provider dispatch reuses the existing `summary::llm_client::generate_summary`,
//! so we don't re-implement provider plumbing; call sites pass its future in.

pub mod prompts;
pub mod providers;
pub mod router;

use router::{RouteTarget, Scope};

/// Why an LLM is being called — used for privacy enforcement and logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Summary,
    Extract,
    Chat,
}

impl Purpose {
    pub fn as_str(&self) -> &'static str {
        match self {
            Purpose::Summary => "summary",
            Purpose::Extract => "extract",
            Purpose::Chat => "chat",
        }
    }
}

/// Privacy configuration (from the settings screen, PLAN.md §8). Enforced here at the
/// provider layer. Defaults are permissive except that nothing overrides `local_only`.
#[derive(Debug, Clone, Copy)]
pub struct PrivacyConfig {
    /// Master switch: when true, NO transcript text leaves the device for any purpose.
    pub local_only: bool,
    pub extraction_enabled: bool,
    pub chat_enabled: bool,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            local_only: false,
            extraction_enabled: true,
            chat_enabled: true,
        }
    }
}

impl PrivacyConfig {
    /// Load privacy settings from `app_settings_kv` (keys `privacy.*`). Missing keys keep
    /// the explicit defaults, but a database/read failure is an error. Outbound operations
    /// must fail closed when the privacy policy cannot be read.
    pub async fn load(pool: &sqlx::SqlitePool) -> Result<Self, LlmError> {
        let mut cfg = Self::default();
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT key, value FROM app_settings_kv WHERE key LIKE 'privacy.%'")
                .fetch_all(pool)
                .await
                .map_err(|e| LlmError::PrivacyConfigUnavailable(e.to_string()))?;
        for (k, v) in rows {
            let on = v == "true" || v == "1";
            match k.as_str() {
                "privacy.local_only" => cfg.local_only = on,
                "privacy.extraction_enabled" => cfg.extraction_enabled = on,
                "privacy.chat_enabled" => cfg.chat_enabled = on,
                _ => {}
            }
        }
        Ok(cfg)
    }

    /// Whether an LLM call for `purpose` is permitted. Returns the specific block reason
    /// so the UI can explain it.
    pub fn ensure_allowed(&self, purpose: Purpose) -> Result<(), LlmError> {
        if self.local_only {
            return Err(LlmError::LocalOnlyMode);
        }
        match purpose {
            Purpose::Extract if !self.extraction_enabled => Err(LlmError::PurposeDisabled(purpose)),
            Purpose::Chat if !self.chat_enabled => Err(LlmError::PurposeDisabled(purpose)),
            _ => Ok(()),
        }
    }
}

#[derive(Debug)]
pub enum LlmError {
    /// Local-only mode is on; the call was blocked before any network activity.
    LocalOnlyMode,
    /// This purpose is disabled in settings.
    PurposeDisabled(Purpose),
    /// Privacy settings could not be read, so the outbound call was denied.
    PrivacyConfigUnavailable(String),
    /// The provider itself failed.
    Provider(String),
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::LocalOnlyMode => write!(f, "local-only mode is enabled; LLM call blocked"),
            LlmError::PurposeDisabled(p) => write!(f, "{} is disabled in settings", p.as_str()),
            LlmError::PrivacyConfigUnavailable(e) => {
                write!(
                    f,
                    "privacy settings unavailable; outbound call blocked: {e}"
                )
            }
            LlmError::Provider(e) => write!(f, "LLM provider error: {e}"),
        }
    }
}

impl std::error::Error for LlmError {}

/// Apply the central privacy policy before resolving credentials or constructing an
/// outbound provider. This is also used by the legacy summary pipeline so it cannot
/// bypass the routed LLM layer.
pub async fn ensure_outbound_allowed(
    pool: &sqlx::SqlitePool,
    purpose: Purpose,
) -> Result<PrivacyConfig, LlmError> {
    let privacy = PrivacyConfig::load(pool).await?;
    privacy.ensure_allowed(purpose)?;
    Ok(privacy)
}

/// Run an LLM completion for `purpose`, enforcing privacy FIRST. `provider_call` is the
/// (lazy) provider future — typically `generate_summary(...)`. It is only awaited if the
/// guard passes, so blocked purposes never touch the network.
pub async fn guarded_complete<F>(
    privacy: &PrivacyConfig,
    purpose: Purpose,
    provider_call: F,
) -> Result<String, LlmError>
where
    F: std::future::Future<Output = Result<String, String>>,
{
    privacy.ensure_allowed(purpose)?;
    log::debug!("llm call permitted: purpose={}", purpose.as_str());
    provider_call.await.map_err(LlmError::Provider)
}

/// High-level entry point: enforce privacy, pick a provider via the router (GigaChat
/// for fast/lookup, DeepSeek for synthesis), and complete — falling back to whichever
/// provider is configured if the routed one isn't. This is what the extract/RAG/summary
/// call sites use, so privacy + routing + provider selection are all centralized.
pub async fn complete_routed(
    pool: &sqlx::SqlitePool,
    purpose: Purpose,
    scope: Scope,
    query_chars: usize,
    system: &str,
    user: &str,
) -> Result<String, LlmError> {
    let privacy = ensure_outbound_allowed(pool, purpose).await?;
    // Guard first — blocked purposes / local-only mode make ZERO network calls and
    // don't even resolve provider credentials.
    let target = router::route(purpose, scope, query_chars);
    let giga = providers::resolve_gigachat(pool).await;
    let deep = providers::resolve_deepseek(pool).await;

    // Preference order depends on the routing decision; fall back to the other.
    match target {
        RouteTarget::Fast => {
            if let Some(g) = &giga {
                return guarded_complete(&privacy, purpose, g.complete(system, user)).await;
            }
            if let Some(d) = &deep {
                return guarded_complete(&privacy, purpose, d.complete(system, user)).await;
            }
        }
        RouteTarget::Synthesis => {
            if let Some(d) = &deep {
                return guarded_complete(&privacy, purpose, d.complete(system, user)).await;
            }
            if let Some(g) = &giga {
                return guarded_complete(&privacy, purpose, g.complete(system, user)).await;
            }
        }
    }
    Err(LlmError::Provider(
        "no LLM provider configured — set GigaChat or DeepSeek credentials in settings".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[test]
    fn guard_blocks_by_purpose_and_local_only() {
        let mut cfg = PrivacyConfig::default();
        assert!(cfg.ensure_allowed(Purpose::Summary).is_ok());
        assert!(cfg.ensure_allowed(Purpose::Extract).is_ok());

        cfg.extraction_enabled = false;
        assert!(matches!(
            cfg.ensure_allowed(Purpose::Extract),
            Err(LlmError::PurposeDisabled(_))
        ));
        assert!(
            cfg.ensure_allowed(Purpose::Summary).is_ok(),
            "summary unaffected"
        );

        cfg = PrivacyConfig {
            local_only: true,
            ..Default::default()
        };
        for p in [Purpose::Summary, Purpose::Extract, Purpose::Chat] {
            assert!(matches!(
                cfg.ensure_allowed(p),
                Err(LlmError::LocalOnlyMode)
            ));
        }
    }

    #[tokio::test]
    async fn blocked_call_never_awaits_provider() {
        let awaited = Arc::new(AtomicBool::new(false));
        let flag = awaited.clone();
        let cfg = PrivacyConfig {
            local_only: true,
            ..Default::default()
        };

        let fut = async move {
            flag.store(true, Ordering::SeqCst);
            Ok::<String, String>("should not run".into())
        };
        let res = guarded_complete(&cfg, Purpose::Chat, fut).await;

        assert!(matches!(res, Err(LlmError::LocalOnlyMode)));
        assert!(
            !awaited.load(Ordering::SeqCst),
            "provider future must NOT be polled when blocked"
        );
    }

    #[tokio::test]
    async fn permitted_call_runs_provider() {
        let cfg = PrivacyConfig::default();
        let res = guarded_complete(&cfg, Purpose::Summary, async {
            Ok::<String, String>("ok".into())
        })
        .await;
        assert_eq!(res.unwrap(), "ok");
    }

    #[tokio::test]
    async fn load_reads_privacy_from_kv() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE app_settings_kv (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        // Missing keys -> defaults.
        assert!(PrivacyConfig::load(&pool).await.unwrap().extraction_enabled);

        sqlx::query("INSERT INTO app_settings_kv(key,value) VALUES('privacy.extraction_enabled','false'),('privacy.local_only','true')")
            .execute(&pool).await.unwrap();
        let cfg = PrivacyConfig::load(&pool).await.unwrap();
        assert!(cfg.local_only && !cfg.extraction_enabled);
        // And the guard then blocks extraction.
        assert!(cfg.ensure_allowed(Purpose::Extract).is_err());
    }

    #[tokio::test]
    async fn load_fails_closed_when_policy_table_is_unavailable() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let err = PrivacyConfig::load(&pool).await.unwrap_err();
        assert!(matches!(err, LlmError::PrivacyConfigUnavailable(_)));
    }
}
