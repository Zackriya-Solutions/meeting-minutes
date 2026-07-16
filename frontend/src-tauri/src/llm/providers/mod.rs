//! Concrete LLM providers for the Russian market (PLAN.md §9 router targets):
//! GigaChat (fast/lookup) and DeepSeek (synthesis). Credentials resolve from
//! `app_settings_kv` first (UI-configurable), then environment variables.

pub mod deepseek;
pub mod gigachat;

use base64::Engine as _;
use sqlx::SqlitePool;

use deepseek::DeepSeekClient;
use gigachat::{GigaChatAuth, GigaChatClient};

#[derive(Clone)]
pub struct DeepSeekTransport {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

fn managed_deepseek_base_url(gateway_base: &str) -> String {
    format!("{}/deepseek/v1", gateway_base.trim_end_matches('/'))
}

/// Read one `app_settings_kv` value (empty strings treated as unset).
async fn kv(pool: &SqlitePool, key: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT value FROM app_settings_kv WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
}

/// Setting value with an environment-variable fallback.
async fn setting_or_env(pool: &SqlitePool, key: &str, env: &str) -> Option<String> {
    if let Some(v) = kv(pool, key).await {
        return Some(v);
    }
    std::env::var(env).ok().filter(|s| !s.is_empty())
}

/// Build a DeepSeek client if an API key is configured (settings `deepseek.api_key`
/// or env `DEEPSEEK_API_KEY`).
pub async fn resolve_deepseek(pool: &SqlitePool) -> Option<DeepSeekClient> {
    let transport = resolve_deepseek_transport(pool).await?;
    Some(DeepSeekClient::new(
        transport.api_key,
        Some(transport.model),
        Some(transport.base_url),
    ))
}

/// Resolve the complete DeepSeek transport, including the gateway-selected base URL.
/// The summary pipeline previously kept only the token and then hard-coded the production
/// gateway, which broke local/server-backed bootstrap configurations.
pub async fn resolve_deepseek_transport(pool: &SqlitePool) -> Option<DeepSeekTransport> {
    let configured_key = setting_or_env(pool, "deepseek.api_key", "DEEPSEEK_API_KEY").await;
    let configured_base = kv(pool, "deepseek.base_url").await;
    let model = kv(pool, "deepseek.model")
        .await
        .unwrap_or_else(|| deepseek::DEFAULT_MODEL.to_string());

    let (api_key, base_url) = match configured_key {
        Some(key) => (
            key,
            configured_base.unwrap_or_else(|| deepseek::DEFAULT_BASE_URL.to_string()),
        ),
        None => {
            let (token, gateway_base) = crate::gateway_identity::install_token().await.ok()?;
            (
                token,
                configured_base.unwrap_or_else(|| managed_deepseek_base_url(&gateway_base)),
            )
        }
    };

    Some(DeepSeekTransport {
        api_key,
        base_url,
        model,
    })
}

#[cfg(test)]
mod tests {
    use super::{managed_deepseek_base_url, resolve_deepseek_transport};
    use sqlx::sqlite::SqlitePoolOptions;

    #[test]
    fn managed_deepseek_keeps_the_gateway_that_accepted_the_token() {
        assert_eq!(
            managed_deepseek_base_url("https://gw2.gigatool.app/"),
            "https://gw2.gigatool.app/deepseek/v1"
        );
    }

    #[tokio::test]
    async fn configured_transport_keeps_custom_base_and_model() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE app_settings_kv(key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO app_settings_kv VALUES \
             ('deepseek.api_key','secret'), \
             ('deepseek.base_url','https://deepseek.example/v1'), \
             ('deepseek.model','deepseek-custom')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let transport = resolve_deepseek_transport(&pool).await.unwrap();
        assert_eq!(transport.api_key, "secret");
        assert_eq!(transport.base_url, "https://deepseek.example/v1");
        assert_eq!(transport.model, "deepseek-custom");
    }
}

/// Build a GigaChat client if credentials are configured. Prefers a single Sber
/// "Authorization Key" (`gigachat.auth_key` / `GIGACHAT_AUTH_KEY`); else user+password
/// (`gigachat.user`+`gigachat.password` / `GIGACHAT_USER`+`GIGACHAT_PASSWORD`).
pub async fn resolve_gigachat(pool: &SqlitePool) -> Option<GigaChatClient> {
    let model = kv(pool, "gigachat.model").await;
    let base = kv(pool, "gigachat.base_url").await;

    if let Some(key) = setting_or_env(pool, "gigachat.auth_key", "GIGACHAT_AUTH_KEY").await {
        return Some(GigaChatClient::new(GigaChatAuth::Key(key), model, base));
    }
    let user = setting_or_env(pool, "gigachat.user", "GIGACHAT_USER").await?;
    let password = setting_or_env(pool, "gigachat.password", "GIGACHAT_PASSWORD").await?;
    Some(GigaChatClient::new(
        GigaChatAuth::UserPassword { user, password },
        model,
        base,
    ))
}

/// Resolve the GigaChat credential as a single Basic-auth key string, for callers
/// (like the meeting-summary pipeline) that only carry one `api_key` value and
/// build a [`GigaChatAuth::Key`] from it. Prefers the Sber "Authorization Key"
/// (`gigachat.auth_key` / `GIGACHAT_AUTH_KEY`); otherwise base64(user:password),
/// which is exactly what HTTP Basic auth expects. Returns None if unconfigured.
pub async fn resolve_gigachat_auth_key(pool: &SqlitePool) -> Option<String> {
    if let Some(key) = setting_or_env(pool, "gigachat.auth_key", "GIGACHAT_AUTH_KEY").await {
        return Some(key);
    }
    let user = setting_or_env(pool, "gigachat.user", "GIGACHAT_USER").await?;
    let password = setting_or_env(pool, "gigachat.password", "GIGACHAT_PASSWORD").await?;
    Some(base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}")))
}

/// Resolve the DeepSeek API key (`deepseek.api_key` / `DEEPSEEK_API_KEY`). None if unset.
pub async fn resolve_deepseek_api_key(pool: &SqlitePool) -> Option<String> {
    resolve_deepseek_transport(pool)
        .await
        .map(|transport| transport.api_key)
}

// NOTE: SaluteSpeech config resolution lives in `crate::salutespeech` (it needs several
// endpoint/model keys with SBER_SALUTE_* env fallbacks), not here.

/// Which providers are configured — for the settings UI / diagnostics.
pub async fn configured(pool: &SqlitePool) -> (bool, bool) {
    (
        resolve_gigachat(pool).await.is_some(),
        resolve_deepseek(pool).await.is_some(),
    )
}
