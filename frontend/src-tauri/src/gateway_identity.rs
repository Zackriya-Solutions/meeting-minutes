//! Per-install identity shared by the managed DeepSeek and SaluteSpeech paths.
//! The gateway JWT is stored in the operating-system credential vault; upstream
//! provider credentials never ship in the application.

use once_cell::sync::{Lazy, OnceCell};
use serde::Serialize;
use std::time::{Duration, Instant};

pub const PRIMARY_GATEWAY_HOST: &str = "gw.multitool.works";
pub const FALLBACK_GATEWAY_HOST: &str = "gw2.multitool.works";
pub const PRIMARY_GATEWAY: &str = "https://gw.multitool.works";
pub const FALLBACK_GATEWAY: &str = "https://gw2.multitool.works";
pub const PRIMARY_DEEPSEEK_BASE_URL: &str = "https://gw.multitool.works/deepseek/v1";
const SERVICE: &str = "meetily.gateway";
static DEVICE_ID_CACHE: OnceCell<String> = OnceCell::new();
static ANALYTICS_DEVICE_ID_ATTEMPT: Lazy<std::sync::Mutex<Option<RetryFailure>>> =
    Lazy::new(|| std::sync::Mutex::new(None));
static GATEWAY_DEVICE_ID_ATTEMPT: Lazy<std::sync::Mutex<Option<RetryFailure>>> =
    Lazy::new(|| std::sync::Mutex::new(None));
static INSTALL_TOKEN_CACHE: Lazy<tokio::sync::RwLock<Option<CachedInstallToken>>> =
    Lazy::new(|| tokio::sync::RwLock::new(None));
static INSTALL_TOKEN_LOCK: Lazy<tokio::sync::Mutex<()>> = Lazy::new(|| tokio::sync::Mutex::new(()));
const INSTALL_TOKEN_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const CREDENTIAL_RETRY_COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct RetryFailure {
    message: String,
    retry_after: Instant,
}

impl RetryFailure {
    fn active_message(&self) -> Option<String> {
        (Instant::now() < self.retry_after).then(|| self.message.clone())
    }
}

#[derive(Clone, Copy)]
enum DeviceIdPurpose {
    Analytics,
    Gateway,
}

fn device_id_attempt(purpose: DeviceIdPurpose) -> &'static std::sync::Mutex<Option<RetryFailure>> {
    match purpose {
        DeviceIdPurpose::Analytics => &ANALYTICS_DEVICE_ID_ATTEMPT,
        DeviceIdPurpose::Gateway => &GATEWAY_DEVICE_ID_ATTEMPT,
    }
}

#[derive(Clone)]
struct CachedInstallToken {
    value: (String, String),
    validated_at: Instant,
}

impl CachedInstallToken {
    fn fresh_value(&self) -> Option<(String, String)> {
        (self.validated_at.elapsed() < INSTALL_TOKEN_CACHE_TTL).then(|| self.value.clone())
    }
}

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

fn entry(name: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, name).map_err(|e| format!("credential vault unavailable: {e}"))
}

fn read_password(name: &str) -> Result<Option<String>, String> {
    match entry(name)?.get_password() {
        Ok(value) if !value.is_empty() => Ok(Some(value)),
        Ok(_) | Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("cannot read {name} from credential vault: {error}")),
    }
}

fn save_password(name: &str, value: &str) -> Result<(), String> {
    entry(name)?
        .set_password(value)
        .map_err(|error| format!("cannot save {name} in credential vault: {error}"))
}

async fn read_password_async(name: &'static str) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || read_password(name))
        .await
        .map_err(|error| format!("credential task failed: {error}"))?
}

async fn save_password_async(name: &'static str, value: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || save_password(name, &value))
        .await
        .map_err(|error| format!("credential task failed: {error}"))?
}

fn device_id_from_vault() -> Result<String, String> {
    if let Some(value) = read_password("device-id")? {
        return Ok(value);
    }
    let value = uuid::Uuid::new_v4().to_string();
    save_password("device-id", &value)?;
    Ok(value)
}

fn device_id(purpose: DeviceIdPurpose) -> Result<String, String> {
    if let Some(value) = DEVICE_ID_CACHE.get() {
        return Ok(value.clone());
    }

    // Serialize the OS dialog and briefly suppress duplicate consumers after a denial.
    // Analytics and gateway registration keep separate failure cooldowns so one feature
    // cannot replay a stale error into the other. Unlike the shared successful device ID,
    // failures are temporary and can recover in the same app process.
    let mut failure = device_id_attempt(purpose)
        .lock()
        .map_err(|_| "credential retry state is unavailable".to_string())?;
    if let Some(value) = DEVICE_ID_CACHE.get() {
        return Ok(value.clone());
    }
    if let Some(message) = failure.as_ref().and_then(RetryFailure::active_message) {
        return Err(message);
    }

    match device_id_from_vault() {
        Ok(value) => {
            let _ = DEVICE_ID_CACHE.set(value.clone());
            *failure = None;
            Ok(value)
        }
        Err(error) => {
            *failure = Some(RetryFailure {
                message: error.clone(),
                retry_after: Instant::now() + CREDENTIAL_RETRY_COOLDOWN,
            });
            Err(error)
        }
    }
}

pub(crate) async fn analytics_device_id() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| device_id(DeviceIdPurpose::Analytics))
        .await
        .map_err(|error| format!("credential task failed: {error}"))?
}

async fn gateway_device_id() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| device_id(DeviceIdPurpose::Gateway))
        .await
        .map_err(|error| format!("credential task failed: {error}"))?
}

/// True when a reply body is an HTML document rather than the gateway's JSON.
///
/// Corporate URL filters and captive portals answer on the gateway's own hostname
/// with an HTML block page, terminating TLS themselves. The client sees a perfectly
/// valid certificate (the proxy's root is in the OS trust store) and an HTTP status
/// that says nothing useful — the observed Sber filter returns **503 with an HTML
/// meta-refresh**. Only the body distinguishes "the gateway is down" from "the
/// request never left the network".
fn looks_like_a_block_page(content_type: &str, body: &str) -> bool {
    content_type.to_ascii_lowercase().contains("html") || body.trim_start().starts_with('<')
}

/// Pull the interceptor's hostname out of a block page, for an error the user can act on.
/// Handles the `<meta http-equiv="refresh" content="0; url=https://host/...">` form.
fn interceptor_host(body: &str) -> Option<String> {
    let rest = body.split("url=https://").nth(1)?;
    let host: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
        .collect();
    (host.len() > 3 && host.contains('.')).then_some(host)
}

/// Decide what a `/register` reply means.
///
/// Split out from the request so every branch is testable without a network, and
/// because the gateway's own failure mode is easy to misread: it answers an invalid
/// registration key with **HTTP 200** and an `{"error": ...}` body, so a status-only
/// check reports the eventual missing-`token` parse failure instead of the rejection.
fn interpret_registration(status: u16, content_type: &str, body: &str) -> Result<String, String> {
    // Checked before the status, because the filter's block page arrives as 503.
    if looks_like_a_block_page(content_type, body) {
        return Err(match interceptor_host(body) {
            Some(host) => format!(
                "запрос к шлюзу Memento перехвачен сетевым фильтром ({host}), \
                 ответ HTTP {status}. Домен шлюза нужно разблокировать в корпоративной сети."
            ),
            None => format!(
                "вместо ответа шлюза Memento пришла HTML-страница (HTTP {status}) — \
                 запрос перехвачен прокси или сетевым фильтром."
            ),
        });
    }

    let parsed: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| format!("шлюз вернул неразбираемый ответ (HTTP {status}): {e}"))?;

    if let Some(error) = parsed.get("error").and_then(serde_json::Value::as_str) {
        return Err(format!(
            "шлюз отклонил регистрацию (HTTP {status}): {error}"
        ));
    }
    if status < 200 || status >= 300 {
        return Err(format!("шлюз ответил ошибкой HTTP {status}"));
    }
    match parsed.get("token").and_then(serde_json::Value::as_str) {
        Some(token) if !token.is_empty() => Ok(token.to_string()),
        _ => Err(format!("ответ шлюза не содержит токена (HTTP {status})")),
    }
}

async fn register(base: &str) -> Result<String, String> {
    let key = registration_key()?;
    let device = gateway_device_id().await?;
    log::info!("[gateway] registering with {base}");

    let response = reqwest::Client::new()
        .post(format!("{}/register", base.trim_end_matches('/')))
        .header("x-memento-registration-key", key)
        .json(&Registration {
            device_id: &device,
            platform: std::env::consts::OS,
            version: env!("CARGO_PKG_VERSION"),
            product: "memento",
        })
        .send()
        .await
        .map_err(|e| format!("нет связи со шлюзом: {e}"))?;

    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = response
        .text()
        .await
        .map_err(|e| format!("не удалось прочитать ответ шлюза: {e}"))?;

    // The caller prefixes the gateway URL, so every failure from here reads
    // "{base}: {reason}" and two gateways failing identically can be collapsed.
    interpret_registration(status, &content_type, &body)
}

/// Whether `/me` still accepts this token. A block page must not read as "valid":
/// a filter that answers 200 with HTML would otherwise keep a dead token in use.
async fn valid(base: &str, token: &str) -> bool {
    let response = match reqwest::Client::new()
        .get(format!("{}/me", base.trim_end_matches('/')))
        .bearer_auth(token)
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => {
            // Request errors are intentionally not interpolated: this module
            // never logs credential-bearing request details.
            log::debug!("[gateway] {base}/me unreachable");
            return false;
        }
    };
    if !response.status().is_success() {
        log::debug!(
            "[gateway] {base}/me rejected the stored token: {}",
            response.status()
        );
        return false;
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = response.text().await.unwrap_or_default();
    if looks_like_a_block_page(&content_type, &body) {
        log::warn!("[gateway] {base}/me answered with a block page, not the gateway");
        return false;
    }
    true
}

/// Register with each gateway in turn, saving the first token that succeeds to the
/// credential vault (overwriting any token already stored).
async fn register_and_store() -> Result<(String, String), String> {
    let mut failures: Vec<String> = Vec::new();
    for base in [PRIMARY_GATEWAY, FALLBACK_GATEWAY] {
        match register(base).await {
            Ok(token) => {
                save_password_async("install-token", token.clone()).await?;
                log::info!("[gateway] registered with {base}");
                return Ok((token, base.to_string()));
            }
            Err(e) => {
                // The detailed failure is returned to the explicit caller,
                // but not copied into logs. Tokens and registration keys are
                // never formatted into an error in this module.
                log::warn!("[gateway] registration failed at {base}");
                failures.push(format!("{base}: {e}"));
            }
        }
    }
    // Both gateways answered the same way in every failure observed so far (they sit
    // behind the same DNS and the same corporate filter), so a single line reads
    // better than two near-identical ones.
    failures.dedup_by(|a, b| after_host(a) == after_host(b));
    Err(if failures.is_empty() {
        "не удалось зарегистрироваться на шлюзе Memento".to_string()
    } else {
        failures.join("; ")
    })
}

/// The part of a failure message after the `"{base}: "` prefix added above. The gateway
/// URLs carry no `": "` of their own (`https://` has no space), so the first one is the
/// separator.
fn after_host(message: &str) -> &str {
    message.split_once(": ").map_or(message, |(_, rest)| rest)
}

/// Return a valid install JWT and the gateway host that accepted it. Reuses the stored
/// token while it still validates against `/me`; otherwise mints a fresh one.
pub async fn install_token() -> Result<(String, String), String> {
    if let Some(cached) = INSTALL_TOKEN_CACHE
        .read()
        .await
        .as_ref()
        .and_then(CachedInstallToken::fresh_value)
    {
        return Ok(cached);
    }
    let _single_flight = INSTALL_TOKEN_LOCK.lock().await;
    if let Some(cached) = INSTALL_TOKEN_CACHE
        .read()
        .await
        .as_ref()
        .and_then(CachedInstallToken::fresh_value)
    {
        return Ok(cached);
    }

    let resolved = if let Some(token) = read_password_async("install-token").await? {
        for base in [PRIMARY_GATEWAY, FALLBACK_GATEWAY] {
            if valid(base, &token).await {
                log::debug!("[gateway] reusing the stored install token against {base}");
                let value = (token, base.to_string());
                *INSTALL_TOKEN_CACHE.write().await = Some(CachedInstallToken {
                    value: value.clone(),
                    validated_at: Instant::now(),
                });
                return Ok(value);
            }
        }
        log::info!("[gateway] the stored install token is no longer accepted, re-registering");
        register_and_store().await
    } else {
        log::debug!("[gateway] no install token stored yet, registering");
        register_and_store().await
    }?;
    *INSTALL_TOKEN_CACHE.write().await = Some(CachedInstallToken {
        value: resolved.clone(),
        validated_at: Instant::now(),
    });
    Ok(resolved)
}

/// Force a brand-new install JWT, discarding the stored one first. [`install_token`]
/// self-heals only when the gateway's `/me` check rejects the cached token; a token that
/// `/me` still accepts but the upstream provider proxy rejects with "invalid or expired
/// token" leaves summary/chat generation stuck. This gives the Settings screen a manual
/// recovery path that always re-registers.
pub async fn force_refresh_token() -> Result<(String, String), String> {
    let _single_flight = INSTALL_TOKEN_LOCK.lock().await;
    let refreshed = register_and_store().await?;
    *INSTALL_TOKEN_CACHE.write().await = Some(CachedInstallToken {
        value: refreshed.clone(),
        validated_at: Instant::now(),
    });
    Ok(refreshed)
}

/// Settings action: mint a fresh managed-gateway token, replacing the stored one. Used to
/// recover an install that hits "invalid or expired token" when generating a summary
/// through the managed DeepSeek path. Fails with a clear message on builds that can't use
/// the managed gateway, or when re-registration fails, rather than the opaque
/// provider-side error.
#[tauri::command]
pub async fn refresh_managed_gateway_token() -> Result<(), String> {
    if !managed_gateway_supported() {
        return Err("This build is not configured to use the managed cloud gateway.".to_string());
    }
    force_refresh_token().await.map(|_| ())
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

    /// Verbatim from a Sber-managed workstation: the corporate URL filter answers on the
    /// gateway's own hostname, over TLS the client accepts, with HTTP 503 and this body.
    const CORPORATE_BLOCK_PAGE: &str = concat!(
        "<html>\n<head>\n<meta http-equiv=\"refresh\" content=\"0; ",
        "url=https://scs-response.sberbank.ru/url_category?cat=parked",
        "&username=sigma\\19539654&url=gw.multitool.works:443/register\">\n</head>\n</html>\n"
    );

    #[test]
    fn a_filter_block_page_names_the_interceptor_instead_of_blaming_the_gateway() {
        let error = interpret_registration(503, "text/html", CORPORATE_BLOCK_PAGE)
            .expect_err("a block page is not a registration");
        assert!(error.contains("scs-response.sberbank.ru"), "{error}");
        assert!(error.contains("перехвачен"), "{error}");
        // The status alone would read as "the gateway is down", which sends people
        // looking at a gateway that is in fact healthy.
        assert!(!error.contains("недоступен"), "{error}");
    }

    #[test]
    fn html_without_a_recognisable_redirect_still_reads_as_interception() {
        let error = interpret_registration(200, "text/html; charset=utf-8", "<html>nope</html>")
            .expect_err("HTML is never a valid registration");
        assert!(error.contains("HTML"), "{error}");
    }

    #[test]
    fn a_rejected_key_is_a_rejection_even_though_the_gateway_answers_200() {
        // The gateway returns 200 with an error body, so a status-only check reports the
        // downstream "missing field `token`" parse failure instead of the real cause.
        let error = interpret_registration(
            200,
            "application/json",
            r#"{"error":"invalid memento registration key"}"#,
        )
        .expect_err("an error body is not a token");
        assert!(
            error.contains("invalid memento registration key"),
            "{error}"
        );
        assert!(error.contains("отклонил"), "{error}");
    }

    #[test]
    fn a_token_is_returned_on_the_normal_path() {
        assert_eq!(
            interpret_registration(200, "application/json", r#"{"token":"jwt-value"}"#).unwrap(),
            "jwt-value"
        );
    }

    #[test]
    fn an_empty_or_absent_token_is_not_accepted() {
        for body in [r#"{"token":""}"#, r#"{"ok":true}"#] {
            let error = interpret_registration(200, "application/json", body).expect_err(body);
            assert!(error.contains("не содержит токена"), "{body} -> {error}");
        }
    }

    #[test]
    fn a_plain_error_status_keeps_its_code() {
        let error = interpret_registration(502, "application/json", r#"{"detail":"upstream"}"#)
            .expect_err("502 is not a registration");
        assert!(error.contains("502"), "{error}");
    }

    #[test]
    fn identical_failures_from_both_gateways_collapse_to_one_line() {
        let a = "https://gw.multitool.works: шлюз ответил ошибкой HTTP 503";
        let b = "https://gw2.multitool.works: шлюз ответил ошибкой HTTP 503";
        assert_eq!(after_host(a), after_host(b));
        assert_eq!(after_host(a), "шлюз ответил ошибкой HTTP 503");
    }

    #[test]
    fn interceptor_host_ignores_bodies_without_a_redirect() {
        assert_eq!(interceptor_host("<html>blocked</html>"), None);
        assert_eq!(
            interceptor_host(CORPORATE_BLOCK_PAGE).as_deref(),
            Some("scs-response.sberbank.ru")
        );
    }

    #[test]
    fn deepseek_base_url_is_scoped_to_the_primary_gateway() {
        assert_managed_https_url(PRIMARY_DEEPSEEK_BASE_URL, PRIMARY_GATEWAY_HOST);
        let url = Url::parse(PRIMARY_DEEPSEEK_BASE_URL).unwrap();
        assert_eq!(url.path(), "/deepseek/v1");
    }

    #[test]
    fn install_token_cache_is_short_lived() {
        let value = ("token".to_string(), PRIMARY_GATEWAY.to_string());
        let fresh = CachedInstallToken {
            value: value.clone(),
            validated_at: Instant::now(),
        };
        assert_eq!(fresh.fresh_value(), Some(value.clone()));

        let expired = CachedInstallToken {
            value,
            validated_at: Instant::now() - INSTALL_TOKEN_CACHE_TTL,
        };
        assert_eq!(expired.fresh_value(), None);
    }

    #[test]
    fn credential_failures_have_a_bounded_retry_cooldown() {
        let active = RetryFailure {
            message: "denied".to_string(),
            retry_after: Instant::now() + CREDENTIAL_RETRY_COOLDOWN,
        };
        assert_eq!(active.active_message().as_deref(), Some("denied"));

        let expired = RetryFailure {
            message: "transient".to_string(),
            retry_after: Instant::now() - Duration::from_millis(1),
        };
        assert_eq!(expired.active_message(), None);
    }

    #[test]
    fn analytics_and_gateway_failures_have_independent_cooldowns() {
        assert!(!std::ptr::eq(
            device_id_attempt(DeviceIdPurpose::Analytics),
            device_id_attempt(DeviceIdPurpose::Gateway),
        ));
    }
}
