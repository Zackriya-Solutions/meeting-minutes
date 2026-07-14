//! GigaChat provider (Sber). Protocol per GigaTool's `sdk/gigachat/` (constants.ts,
//! token.ts, fetch.ts):
//!   1. Mint a short-lived OAuth access token: `POST {base}/token` with Basic auth
//!      (the Sber "Authorization Key" = base64(client_id:client_secret), or
//!      user:password) + an `RqUID` header. Response: `{tok|access_token|token, exp}`.
//!   2. Chat completions: `POST {base}/chat/completions` with `Bearer {token}`,
//!      OpenAI-shaped body/response.
//! Tokens are cached until ~60s before expiry. Plain system+user→text needs none of
//! GigaTool's tool-call/legacy-`functions` translation.

use std::time::{Duration, SystemTime};

use serde_json::json;
use tokio::sync::Mutex;

use super::deepseek::extract_openai_content;

pub const DEFAULT_BASE_URL: &str = "https://gigachat.sberdevices.ru/v1";
pub const DEFAULT_MODEL: &str = "GigaChat-3-Ultra";
/// Sber's WAF flags requests without a recognizable `GigaChat-*` User-Agent.
pub const USER_AGENT: &str = "GigaChat-Meetily";

/// How GigaChat credentials are supplied.
#[derive(Clone)]
pub enum GigaChatAuth {
    /// Pre-encoded Sber "Authorization Key" (base64(client_id:client_secret)).
    Key(String),
    /// Separate user/password (base64-encoded by reqwest's basic_auth).
    UserPassword { user: String, password: String },
}

struct CachedToken {
    value: String,
    expires_at: SystemTime,
}

pub struct GigaChatClient {
    client: reqwest::Client,
    auth: GigaChatAuth,
    base_url: String,
    model: String,
    token: Mutex<Option<CachedToken>>,
}

impl GigaChatClient {
    pub fn new(auth: GigaChatAuth, model: Option<String>, base_url: Option<String>) -> Self {
        Self {
            // Sber's cert chain is a Russian CA; if a user's system trust store lacks it,
            // TLS fails. We do NOT disable verification here (that would be insecure) —
            // document that users must trust the Russian Ministry CA. accept_invalid_certs
            // is intentionally left off.
            client: reqwest::Client::new(),
            auth,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            token: Mutex::new(None),
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// Return a valid access token, minting a new one if the cache is empty or within
    /// 60s of expiry.
    async fn access_token(&self) -> Result<String, String> {
        let mut guard = self.token.lock().await;
        if let Some(cached) = guard.as_ref() {
            if cached
                .expires_at
                .duration_since(SystemTime::now())
                .map(|left| left > Duration::from_secs(60))
                .unwrap_or(false)
            {
                return Ok(cached.value.clone());
            }
        }

        let (token, expires_at) = self.mint_token().await?;
        *guard = Some(CachedToken {
            value: token.clone(),
            expires_at,
        });
        Ok(token)
    }

    async fn mint_token(&self) -> Result<(String, SystemTime), String> {
        let url = format!("{}/token", self.base_url.trim_end_matches('/'));
        let mut req = self
            .client
            .post(&url)
            .header("RqUID", uuid::Uuid::new_v4().to_string())
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::USER_AGENT, USER_AGENT);

        req = match &self.auth {
            GigaChatAuth::Key(key) => {
                req.header(reqwest::header::AUTHORIZATION, format!("Basic {key}"))
            }
            GigaChatAuth::UserPassword { user, password } => req.basic_auth(user, Some(password)),
        };

        let resp = req
            .send()
            .await
            .map_err(|e| format!("gigachat token request failed: {e}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!(
                "gigachat token error {status}: {}",
                text.chars().take(300).collect::<String>()
            ));
        }

        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("gigachat token parse failed: {e}"))?;
        let token = v
            .get("tok")
            .or_else(|| v.get("access_token"))
            .or_else(|| v.get("token"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| "gigachat token response missing token field".to_string())?
            .to_string();

        let expires_at = parse_expiry(&v);
        Ok((token, expires_at))
    }

    /// Run a system+user completion, returning the assistant text.
    pub async fn complete(&self, system: &str, user: &str) -> Result<String, String> {
        let token = self.access_token().await?;
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "stream": false,
        });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::ACCEPT, "application/json")
            .header("X-Request-ID", uuid::Uuid::new_v4().to_string())
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("gigachat request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!(
                "gigachat error {status}: {}",
                text.chars().take(300).collect::<String>()
            ));
        }

        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("gigachat response parse failed: {e}"))?;
        extract_openai_content(&v)
            .ok_or_else(|| "gigachat response missing choices[0].message.content".to_string())
    }
}

/// Compute token expiry from the response. `exp`/`expires_at` may be unix seconds or
/// milliseconds (values > 1e12 are treated as ms, matching GigaTool's heuristic).
/// Falls back to now + 25 minutes when absent.
fn parse_expiry(v: &serde_json::Value) -> SystemTime {
    let raw = v
        .get("exp")
        .or_else(|| v.get("expires_at"))
        .and_then(|e| e.as_f64());
    match raw {
        Some(r) if r > 0.0 => {
            let secs = if r > 1e12 { r / 1000.0 } else { r };
            SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64)
        }
        _ => SystemTime::now() + Duration::from_secs(25 * 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expiry_handles_seconds_and_millis() {
        // seconds
        let secs = parse_expiry(&json!({"exp": 2_000_000_000u64}));
        assert!(secs > SystemTime::UNIX_EPOCH + Duration::from_secs(1_999_999_000));
        // milliseconds (>1e12) get divided
        let ms = parse_expiry(&json!({"expires_at": 2_000_000_000_000u64}));
        assert!(ms > SystemTime::UNIX_EPOCH + Duration::from_secs(1_999_999_000));
        // missing -> ~now + 25 min (in the future)
        assert!(parse_expiry(&json!({})) > SystemTime::now() + Duration::from_secs(60));
    }
}
