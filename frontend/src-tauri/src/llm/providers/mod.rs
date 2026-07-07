//! Concrete LLM providers for the Russian market (PLAN.md §9 router targets):
//! GigaChat (fast/lookup) and DeepSeek (synthesis). Credentials resolve from
//! `app_settings_kv` first (UI-configurable), then environment variables.

pub mod deepseek;
pub mod gigachat;

use base64::Engine as _;
use sqlx::SqlitePool;

use deepseek::DeepSeekClient;
use gigachat::{GigaChatAuth, GigaChatClient};

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
    let api_key = setting_or_env(pool, "deepseek.api_key", "DEEPSEEK_API_KEY").await?;
    Some(DeepSeekClient::new(
        api_key,
        kv(pool, "deepseek.model").await,
        kv(pool, "deepseek.base_url").await,
    ))
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
    setting_or_env(pool, "deepseek.api_key", "DEEPSEEK_API_KEY").await
}

/// Which providers are configured — for the settings UI / diagnostics.
pub async fn configured(pool: &SqlitePool) -> (bool, bool) {
    (
        resolve_gigachat(pool).await.is_some(),
        resolve_deepseek(pool).await.is_some(),
    )
}
