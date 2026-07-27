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
    pub max_tokens: u32,
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

fn deepseek_max_tokens(value: Option<String>) -> u32 {
    value
        .and_then(|raw| raw.trim().parse::<u32>().ok())
        .filter(|tokens| (deepseek::MIN_MAX_TOKENS..=deepseek::MAX_MAX_TOKENS).contains(tokens))
        .unwrap_or(deepseek::DEFAULT_MAX_TOKENS)
}

/// Build a DeepSeek client, or explain why it cannot be built.
///
/// The error is user-facing: callers previously collapsed every failure into `None` and
/// printed "add an API key", which is wrong for the managed path — it has no key to add,
/// and the actual causes (gateway blocked by a network filter, registration rejected)
/// were invisible. Keep the reason attached all the way to the UI.
pub async fn resolve_deepseek(pool: &SqlitePool) -> Result<DeepSeekClient, String> {
    let transport = resolve_deepseek_transport(pool).await?;
    Ok(DeepSeekClient::new(
        transport.api_key,
        Some(transport.model),
        Some(transport.base_url),
        Some(transport.max_tokens),
    ))
}

/// Resolve the complete DeepSeek transport, including the gateway-selected base URL.
/// The summary pipeline previously kept only the token and then hard-coded the production
/// gateway, which broke local/server-backed bootstrap configurations.
pub async fn resolve_deepseek_transport(pool: &SqlitePool) -> Result<DeepSeekTransport, String> {
    let configured_key = setting_or_env(pool, "deepseek.api_key", "DEEPSEEK_API_KEY").await;
    let configured_base = kv(pool, "deepseek.base_url").await;
    let model = kv(pool, "deepseek.model")
        .await
        .unwrap_or_else(|| deepseek::DEFAULT_MODEL.to_string());
    let max_tokens = deepseek_max_tokens(
        setting_or_env(pool, "deepseek.max_tokens", "DEEPSEEK_MAX_TOKENS").await,
    );

    let (api_key, base_url) = match configured_key {
        Some(key) => (
            key,
            configured_base.unwrap_or_else(|| deepseek::DEFAULT_BASE_URL.to_string()),
        ),
        None => {
            let (token, gateway_base) = crate::gateway_identity::install_token()
                .await
                .map_err(|e| format!("Управляемый шлюз Memento недоступен: {e}"))?;
            (
                token,
                configured_base.unwrap_or_else(|| managed_deepseek_base_url(&gateway_base)),
            )
        }
    };

    Ok(DeepSeekTransport {
        api_key,
        base_url,
        model,
        max_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::{deepseek_max_tokens, managed_deepseek_base_url, resolve_deepseek_transport};
    use crate::llm::providers::deepseek;
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
             ('deepseek.model','deepseek-custom'), \
             ('deepseek.max_tokens','16384')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let transport = resolve_deepseek_transport(&pool).await.unwrap();
        assert_eq!(transport.api_key, "secret");
        assert_eq!(transport.base_url, "https://deepseek.example/v1");
        assert_eq!(transport.model, "deepseek-custom");
        assert_eq!(transport.max_tokens, 16_384);
    }

    #[test]
    fn deepseek_output_budget_is_bounded_and_has_a_safe_default() {
        assert_eq!(deepseek_max_tokens(None), deepseek::DEFAULT_MAX_TOKENS);
        assert_eq!(deepseek_max_tokens(Some("16384".to_string())), 16_384);
        assert_eq!(
            deepseek_max_tokens(Some("999999".to_string())),
            deepseek::DEFAULT_MAX_TOKENS
        );
        assert_eq!(
            deepseek_max_tokens(Some("invalid".to_string())),
            deepseek::DEFAULT_MAX_TOKENS
        );
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
        .ok()
}

// NOTE: SaluteSpeech config resolution lives in `crate::salutespeech` (it needs several
// endpoint/model keys with SBER_SALUTE_* env fallbacks), not here.

/// Which providers are configured — for the settings UI / diagnostics.
pub async fn configured(pool: &SqlitePool) -> (bool, bool) {
    (
        resolve_gigachat(pool).await.is_some(),
        resolve_deepseek(pool).await.is_ok(),
    )
}
