//! Per-install identity shared by the managed DeepSeek and SaluteSpeech paths.
//! The gateway JWT is stored in the operating-system credential vault; upstream
//! provider credentials never ship in the application.

use serde::{Deserialize, Serialize};

pub const PRIMARY_GATEWAY_HOST: &str = "gw.multitool.works";
pub const FALLBACK_GATEWAY_HOST: &str = "gw2.multitool.works";
pub const PRIMARY_GATEWAY: &str = "https://gw.multitool.works";
pub const FALLBACK_GATEWAY: &str = "https://gw2.multitool.works";
pub const PRIMARY_DEEPSEEK_BASE_URL: &str = "https://gw.multitool.works/deepseek/v1";
const SERVICE: &str = "meetily.gateway";

fn registration_key() -> Result<String, String> {
    // Release builds receive this at compile time. Runtime env is kept for local
    // development/CI. The value is never committed to the repository.
    option_env!("MEMENTO_REGISTRATION_KEY")
        .map(str::to_owned)
        .or_else(|| std::env::var("MEMENTO_REGISTRATION_KEY").ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "MEMENTO_REGISTRATION_KEY is missing from this build".to_string())
}

/// Whether this binary can register with the managed Memento gateway.
///
/// This is a local capability check only: it does not access the network or the
/// credential vault. Callers use it to avoid offering a cloud migration that
/// can never succeed in a development or unsigned test build.
pub fn managed_gateway_supported() -> bool {
    registration_key().is_ok()
}

#[derive(Serialize)]
struct Registration<'a> {
    #[serde(rename = "deviceId")]
    device_id: &'a str,
    platform: &'a str,
    version: &'a str,
    product: &'a str,
}

#[derive(Deserialize)]
struct RegistrationResponse {
    token: String,
}

fn entry(name: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, name).map_err(|e| format!("credential vault unavailable: {e}"))
}

fn device_id() -> Result<String, String> {
    let item = entry("device-id")?;
    if let Ok(value) = item.get_password() {
        if !value.is_empty() {
            return Ok(value);
        }
    }
    let value = uuid::Uuid::new_v4().to_string();
    item.set_password(&value)
        .map_err(|e| format!("cannot save gateway device id: {e}"))?;
    Ok(value)
}

async fn register(base: &str) -> Result<String, String> {
    let response = reqwest::Client::new()
        .post(format!("{}/register", base.trim_end_matches('/')))
        .header("x-memento-registration-key", registration_key()?)
        .json(&Registration {
            device_id: &device_id()?,
            platform: std::env::consts::OS,
            version: env!("CARGO_PKG_VERSION"),
            product: "memento",
        })
        .send()
        .await
        .map_err(|e| format!("gateway registration failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("gateway registration error {}", response.status()));
    }
    response
        .json::<RegistrationResponse>()
        .await
        .map(|v| v.token)
        .map_err(|e| format!("invalid gateway registration response: {e}"))
}

async fn valid(base: &str, token: &str) -> bool {
    reqwest::Client::new()
        .get(format!("{}/me", base.trim_end_matches('/')))
        .bearer_auth(token)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Return a valid install JWT and the gateway host that accepted it.
pub async fn install_token() -> Result<(String, String), String> {
    let item = entry("install-token")?;
    if let Ok(token) = item.get_password() {
        for base in [PRIMARY_GATEWAY, FALLBACK_GATEWAY] {
            if valid(base, &token).await {
                return Ok((token, base.to_string()));
            }
        }
    }
    let mut last = String::new();
    for base in [PRIMARY_GATEWAY, FALLBACK_GATEWAY] {
        match register(base).await {
            Ok(token) => {
                item.set_password(&token)
                    .map_err(|e| format!("cannot save gateway token: {e}"))?;
                return Ok((token, base.to_string()));
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Url;

    fn assert_managed_https_url(value: &str, expected_host: &str) {
        let url = Url::parse(value).expect("managed gateway URL must parse");
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some(expected_host));
        assert!(url.username().is_empty());
        assert!(url.password().is_none());
        assert!(url.query().is_none());
        assert!(url.fragment().is_none());
    }

    #[test]
    fn managed_gateway_domains_are_exact_https_allowlist_entries() {
        assert_managed_https_url(PRIMARY_GATEWAY, PRIMARY_GATEWAY_HOST);
        assert_managed_https_url(FALLBACK_GATEWAY, FALLBACK_GATEWAY_HOST);
        assert_ne!(PRIMARY_GATEWAY, FALLBACK_GATEWAY);
    }

    #[test]
    fn deepseek_base_url_is_scoped_to_the_primary_gateway() {
        assert_managed_https_url(PRIMARY_DEEPSEEK_BASE_URL, PRIMARY_GATEWAY_HOST);
        let url = Url::parse(PRIMARY_DEEPSEEK_BASE_URL).unwrap();
        assert_eq!(url.path(), "/deepseek/v1");
    }
}
