//! SaluteSpeech OAuth token client.
//!
//! Mints a short-lived Bearer token from the "Authorization Key" (HTTP Basic) against
//! the configured token endpoint (default the `speech.giga.chat` gateway, which takes
//! `base64(login:password)`), caching it until ~60s before expiry. Verified live: the
//! gateway returns `{"tok": "...", "exp": <unix seconds>}`. A `scope` is sent only when
//! configured (the raw `ngw.devices.sberbank.ru` endpoint needs it; the gateway ignores
//! it).

use std::time::{Duration, SystemTime};

use tokio::sync::Mutex;

struct CachedToken {
    value: String,
    expires_at: SystemTime,
}

/// Mints + caches SaluteSpeech access tokens.
pub struct SaluteSpeechAuth {
    client: reqwest::Client,
    auth_key: String,
    oauth_url: String,
    scope: Option<String>,
    token: Mutex<Option<CachedToken>>,
}

impl SaluteSpeechAuth {
    pub fn new(auth_key: String, oauth_url: String, scope: Option<String>) -> Self {
        Self {
            // System trust roots verify Sber's chain (checked live); verification stays ON.
            client: reqwest::Client::new(),
            auth_key,
            oauth_url,
            scope: scope.filter(|s| !s.trim().is_empty()),
            token: Mutex::new(None),
        }
    }

    /// Return a valid access token, minting a fresh one if the cache is empty or within
    /// 60s of expiry.
    pub async fn access_token(&self) -> Result<String, String> {
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
        let managed_gateway = self.oauth_url.ends_with("/salutespeech/token");
        let mut req = self
            .client
            .post(&self.oauth_url)
            .header("RqUID", uuid::Uuid::new_v4().to_string())
            .header(
                reqwest::header::AUTHORIZATION,
                format!(
                    "{} {}",
                    if managed_gateway { "Bearer" } else { "Basic" },
                    self.auth_key
                ),
            )
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::USER_AGENT, "GigaChat-Meetily");

        if !managed_gateway {
            if let Some(scope) = &self.scope {
                req = req.form(&[("scope", scope.as_str())]);
            }
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("salutespeech token request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!(
                "salutespeech token error {status}: {}",
                text.chars().take(300).collect::<String>()
            ));
        }

        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("salutespeech token parse failed: {e}"))?;
        let token = v
            .get("tok")
            .or_else(|| v.get("access_token"))
            .or_else(|| v.get("token"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| "salutespeech token response missing token field".to_string())?
            .to_string();

        Ok((token, parse_expiry(&v)))
    }
}

/// Compute token expiry. Sber returns `exp`/`expires_at` as a unix timestamp; values
/// larger than 1e12 are milliseconds. Falls back to now + 25 min when absent.
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
    use serde_json::json;

    #[test]
    fn expiry_handles_seconds_and_millis() {
        // giga.chat returns unix seconds (e.g. 1783610797)
        let secs = parse_expiry(&json!({"exp": 1_783_610_797u64}));
        assert!(secs > SystemTime::UNIX_EPOCH + Duration::from_secs(1_783_610_000));
        let ms = parse_expiry(&json!({"expires_at": 2_000_000_000_000u64}));
        assert!(ms > SystemTime::UNIX_EPOCH + Duration::from_secs(1_999_999_000));
        assert!(parse_expiry(&json!({})) > SystemTime::now() + Duration::from_secs(60));
    }

    #[test]
    fn blank_scope_is_dropped() {
        let a = SaluteSpeechAuth::new("k".into(), "https://x/token".into(), Some("  ".into()));
        assert!(a.scope.is_none());
        let b = SaluteSpeechAuth::new(
            "k".into(),
            "https://x/token".into(),
            Some("SALUTE_SPEECH_PERS".into()),
        );
        assert_eq!(b.scope.as_deref(), Some("SALUTE_SPEECH_PERS"));
    }
}
