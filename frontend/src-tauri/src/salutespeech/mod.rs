//! SaluteSpeech (Sber) cloud ASR via the **synchronous HTTP recognize API**.
//!
//! Reached through the GigaChat-branded gateway `speech.giga.chat`: an OAuth token is
//! minted from the "Authorization Key" (`base64(login:password)`), then each speech
//! segment is POSTed to `/rest/v1/speech:recognize` and the transcript comes back in
//! one response. Because it's segment-at-a-time, it slots straight into the shared
//! [`crate::audio::transcription::TranscriptionProvider`] trait — so the same code path
//! serves live recording, file import, and re-transcription. (Verified end-to-end
//! against the live endpoint: 16 kHz PCM → exact Russian transcript.)
//!
//! Endpoints, model, and scope are configurable via `app_settings_kv`
//! (`salutespeech.*`) or the environment (`SBER_SALUTE_*` / `SALUTESPEECH_*`), with
//! `speech.giga.chat` + model `universal_turbo` as defaults. TLS verifies with system
//! trust roots (no custom CA needed). Speaker attribution comes from the app's existing
//! local diarization / channel tagging — the sync REST API returns no speaker labels.

pub mod auth;
pub mod diarize;
pub mod rest;

pub use rest::SaluteSpeechProvider;

use sqlx::SqlitePool;

pub const DEFAULT_OAUTH_URL: &str = "https://speech.giga.chat/v1/token";
pub const DEFAULT_RECOGNIZE_URL: &str = "https://speech.giga.chat/rest/v1/speech:recognize";
/// giga.chat recognition models: transcribation_hq | universal_turbo.
pub const DEFAULT_MODEL: &str = "universal_turbo";

/// Resolved SaluteSpeech configuration for a recording/session.
#[derive(Clone, Debug)]
pub struct SaluteSpeechConfig {
    /// Sber "Authorization Key" = `base64(login:password)`, used as HTTP Basic.
    pub auth_key: String,
    /// OAuth scope (only the raw `ngw` endpoint needs it; the gateway ignores it).
    pub scope: Option<String>,
    pub oauth_url: String,
    pub recognize_url: String,
    pub model: String,
}

async fn kv(pool: &SqlitePool, key: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT value FROM app_settings_kv WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.trim().is_empty())
}

/// Setting → env fallbacks → default.
async fn resolve(pool: &SqlitePool, key: &str, envs: &[&str], default: &str) -> String {
    if let Some(v) = kv(pool, key).await {
        return v;
    }
    for e in envs {
        if let Some(v) = env(e) {
            return v;
        }
    }
    default.to_string()
}

/// Resolve full config from settings/env. `None` when no Authorization Key is set.
pub async fn resolve_config(pool: &SqlitePool) -> Option<SaluteSpeechConfig> {
    let configured_key = kv(pool, "salutespeech.auth_key")
        .await
        .or_else(|| env("SALUTESPEECH_AUTH_KEY"))
        .or_else(|| env("SBER_SALUTE_AUTH_KEY"));
    let managed = if configured_key.is_none() {
        crate::gateway_identity::install_token().await.ok()
    } else {
        None
    };
    let auth_key = configured_key.or_else(|| managed.as_ref().map(|v| v.0.clone()))?;
    let scope = kv(pool, "salutespeech.scope")
        .await
        .or_else(|| env("SALUTESPEECH_SCOPE"))
        .or_else(|| env("SBER_SALUTE_SCOPE"));
    let oauth_url = match managed {
        Some((_, base)) => format!("{}/salutespeech/token", base.trim_end_matches('/')),
        None => {
            resolve(
                pool,
                "salutespeech.oauth_url",
                &["SBER_SALUTE_OAUTH_URL"],
                DEFAULT_OAUTH_URL,
            )
            .await
        }
    };
    let recognize_url = resolve(
        pool,
        "salutespeech.recognize_url",
        &["SBER_SALUTE_RECOGNIZE_URL"],
        DEFAULT_RECOGNIZE_URL,
    )
    .await;
    let model = resolve(
        pool,
        "salutespeech.model",
        &["SBER_SALUTE_RECOGNITION_MODEL"],
        DEFAULT_MODEL,
    )
    .await;

    Some(SaluteSpeechConfig {
        auth_key,
        scope,
        oauth_url,
        recognize_url,
        model,
    })
}

/// Whether SaluteSpeech is usable (an Authorization Key is configured).
pub async fn is_configured(pool: &SqlitePool) -> bool {
    resolve_config(pool).await.is_some()
}

/// Map the app's language preference (`"ru"`, `"en"`, `"auto"`, `None`) to a
/// SaluteSpeech RFC-3066 language code (ru-RU, en-US, kk-KZ, ky-KG, uz-UZ).
/// Defaults to Russian.
pub fn map_language(pref: Option<String>) -> String {
    let p = pref.unwrap_or_default().to_lowercase();
    let code = p.split(['-', '_']).next().unwrap_or("");
    match code {
        "en" => "en-US",
        "kk" => "kk-KZ",
        "ky" => "ky-KG",
        "uz" => "uz-UZ",
        _ => "ru-RU",
    }
    .to_string()
}
