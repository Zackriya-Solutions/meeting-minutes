// Google OAuth 2.0 loopback flow ("installed app" flow, RFC 8252) for Calendar access.
//
// No OAuth plugin/dependency exists elsewhere in this codebase, so the redirect is caught
// with a minimal single-request TCP listener on an OS-assigned 127.0.0.1 port instead of
// pulling in a new crate. Desktop-type Google OAuth clients don't require pre-registering
// that port, so this works without any redirect_uri allowlist configuration on Google's side.

use super::GoogleCalendarConfig;
use anyhow::{anyhow, Result};
use chrono::{Duration, Utc};
use log::{error, info};
use serde::Deserialize;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

pub const CALENDAR_SCOPE: &str = "https://www.googleapis.com/auth/calendar.readonly";
const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: i64,
    #[serde(default)]
    scope: Option<String>,
}

pub fn build_authorize_url(client_id: &str, redirect_uri: &str) -> String {
    let mut url = url::Url::parse(AUTH_ENDPOINT).expect("AUTH_ENDPOINT is a valid URL");
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", CALENDAR_SCOPE)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");
    url.to_string()
}

/// Bind the loopback listener up front so the port is known before the browser opens.
pub fn bind_loopback_listener() -> Result<(TcpListener, u16)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

/// Blocks until Google's redirect delivers an authorization code, or `timeout` elapses.
/// Must run in a blocking context (e.g. `tokio::task::spawn_blocking`).
pub fn wait_for_authorization_code(
    listener: TcpListener,
    timeout: std::time::Duration,
) -> Result<String> {
    listener.set_nonblocking(true)?;
    let deadline = std::time::Instant::now() + timeout;

    loop {
        if std::time::Instant::now() > deadline {
            return Err(anyhow!("Timed out waiting for Google OAuth redirect"));
        }

        match listener.accept() {
            Ok((mut stream, _addr)) => {
                stream.set_nonblocking(false)?;
                let mut reader = BufReader::new(stream.try_clone()?);
                let mut request_line = String::new();
                reader.read_line(&mut request_line)?;

                // "GET /callback?code=XYZ&scope=... HTTP/1.1"
                let path = request_line.split_whitespace().nth(1).unwrap_or("");
                let query = path.split_once('?').map(|(_, q)| q).unwrap_or("");
                let code = query
                    .split('&')
                    .find_map(|pair| pair.strip_prefix("code=").map(|c| c.to_string()));

                let body = "<html><body>Meetily is connected to Google Calendar. \
                             You can close this tab.</body></html>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());

                if let Some(code) = code {
                    return Ok(code);
                }
                // No code on this request (e.g. a stray favicon.ico fetch) — keep waiting.
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

pub async fn exchange_code_for_tokens(
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
) -> Result<GoogleCalendarConfig> {
    let client = reqwest::Client::new();
    let params = [
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("code", code),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri),
    ];

    let response = client.post(TOKEN_ENDPOINT).form(&params).send().await?;
    if !response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        error!("Google token exchange failed: {}", text);
        return Err(anyhow!("Google token exchange failed: {}", text));
    }

    let token: TokenResponse = response.json().await?;
    let refresh_token = token.refresh_token.ok_or_else(|| {
        anyhow!(
            "Google did not return a refresh_token. If you've connected before, revoke \
             Meetily's access at https://myaccount.google.com/permissions and try again."
        )
    })?;

    Ok(GoogleCalendarConfig {
        client_id: client_id.to_string(),
        client_secret: client_secret.to_string(),
        access_token: Some(token.access_token),
        refresh_token: Some(refresh_token),
        token_expiry: Some((Utc::now() + Duration::seconds(token.expires_in)).to_rfc3339()),
        scope: token.scope,
    })
}

pub async fn refresh_access_token(config: &GoogleCalendarConfig) -> Result<GoogleCalendarConfig> {
    let refresh_token = config
        .refresh_token
        .as_ref()
        .ok_or_else(|| anyhow!("No refresh_token stored; reconnect Google Calendar"))?;

    let client = reqwest::Client::new();
    let params = [
        ("client_id", config.client_id.as_str()),
        ("client_secret", config.client_secret.as_str()),
        ("refresh_token", refresh_token.as_str()),
        ("grant_type", "refresh_token"),
    ];

    let response = client.post(TOKEN_ENDPOINT).form(&params).send().await?;
    if !response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        error!("Google token refresh failed: {}", text);
        return Err(anyhow!("Google token refresh failed: {}", text));
    }

    let token: TokenResponse = response.json().await?;

    Ok(GoogleCalendarConfig {
        client_id: config.client_id.clone(),
        client_secret: config.client_secret.clone(),
        access_token: Some(token.access_token),
        refresh_token: Some(
            token
                .refresh_token
                .unwrap_or_else(|| refresh_token.clone()),
        ),
        token_expiry: Some((Utc::now() + Duration::seconds(token.expires_in)).to_rfc3339()),
        scope: token.scope.or_else(|| config.scope.clone()),
    })
}

/// Returns a valid access token, refreshing (and persisting) it first if it's expired
/// or about to expire.
pub async fn ensure_valid_access_token(
    pool: &sqlx::SqlitePool,
    config: &GoogleCalendarConfig,
) -> Result<String> {
    let needs_refresh = match &config.token_expiry {
        Some(expiry) => {
            let expiry = chrono::DateTime::parse_from_rfc3339(expiry)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            expiry <= Utc::now() + Duration::minutes(2)
        }
        None => true,
    };

    if !needs_refresh {
        return config
            .access_token
            .clone()
            .ok_or_else(|| anyhow!("No access_token stored; reconnect Google Calendar"));
    }

    info!("Refreshing Google Calendar access token");
    let refreshed = refresh_access_token(config).await?;
    let access_token = refreshed
        .access_token
        .clone()
        .ok_or_else(|| anyhow!("Google refresh response did not include an access_token"))?;

    crate::database::repositories::setting::SettingsRepository::save_google_calendar_config(
        pool, &refreshed,
    )
    .await
    .map_err(|e| anyhow!("Failed to persist refreshed Google token: {}", e))?;

    Ok(access_token)
}
