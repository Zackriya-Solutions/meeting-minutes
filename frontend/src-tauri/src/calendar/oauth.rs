use super::models::{GoogleTokenResponse, GoogleUserInfo};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Read-only Calendar access, plus the (non-sensitive, no extra consent-screen config
/// needed) `userinfo.email` scope — required for the userinfo endpoint to identify which
/// account connected, otherwise it 401s even with a perfectly valid Calendar-scoped token.
pub const SCOPE: &str =
    "https://www.googleapis.com/auth/calendar.events.readonly https://www.googleapis.com/auth/userinfo.email";

const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const USERINFO_ENDPOINT: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);

fn client_id() -> Result<&'static str, String> {
    option_env!("GOOGLE_CALENDAR_CLIENT_ID")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Google Calendar client not configured (missing GOOGLE_CALENDAR_CLIENT_ID at build time)".to_string())
}

fn client_secret() -> Result<&'static str, String> {
    option_env!("GOOGLE_CALENDAR_CLIENT_SECRET")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Google Calendar client not configured (missing GOOGLE_CALENDAR_CLIENT_SECRET at build time)".to_string())
}

pub struct PkceChallenge {
    pub verifier: String,
    pub challenge: String,
}

fn random_url_safe_token(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn generate_pkce() -> PkceChallenge {
    let verifier = random_url_safe_token(64);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    PkceChallenge { verifier, challenge }
}

pub fn generate_state() -> String {
    random_url_safe_token(24)
}

/// Binds an ephemeral loopback port for the OAuth redirect. Bind before building the
/// auth URL so the exact port is known (Google's Desktop-app client type accepts any
/// `http://127.0.0.1:<port>/...` redirect without pre-registration).
pub async fn bind_loopback_listener() -> Result<(TcpListener, u16), String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Failed to open local OAuth callback port: {}", e))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Failed to read local OAuth callback port: {}", e))?
        .port();
    Ok((listener, port))
}

pub fn redirect_uri(port: u16) -> String {
    format!("http://127.0.0.1:{}/callback", port)
}

pub fn build_auth_url(port: u16, state: &str, code_challenge: &str) -> Result<String, String> {
    let url = url::Url::parse_with_params(
        AUTH_ENDPOINT,
        &[
            ("client_id", client_id()?),
            ("redirect_uri", &redirect_uri(port)),
            ("response_type", "code"),
            ("scope", SCOPE),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("code_challenge", code_challenge),
            ("code_challenge_method", "S256"),
            ("state", state),
        ],
    )
    .map_err(|e| format!("Failed to build Google auth URL: {}", e))?;
    Ok(url.to_string())
}

const CALLBACK_PAGE_BODY: &str = "<html><body><h3>Meetily</h3><p>Google Calendar connected. You can close this tab and return to the app.</p></body></html>";

/// Accepts exactly one loopback connection carrying the OAuth redirect, extracts and
/// validates `code`/`state`, replies with a static page, then shuts down.
pub async fn await_callback(listener: TcpListener, expected_state: &str) -> Result<String, String> {
    let accept = tokio::time::timeout(CALLBACK_TIMEOUT, listener.accept());
    let (mut stream, _) = accept
        .await
        .map_err(|_| "Timed out waiting for Google sign-in (5 min)".to_string())?
        .map_err(|e| format!("Failed to accept OAuth callback connection: {}", e))?;

    let mut buf = vec![0u8; 8192];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| format!("Failed to read OAuth callback request: {}", e))?;
    let request = String::from_utf8_lossy(&buf[..n]);

    let request_line = request
        .lines()
        .next()
        .ok_or_else(|| "Empty OAuth callback request".to_string())?;
    let path = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| "Malformed OAuth callback request line".to_string())?;

    let full_url = url::Url::parse(&format!("http://127.0.0.1{}", path))
        .map_err(|e| format!("Failed to parse OAuth callback URL: {}", e))?;

    let mut code = None;
    let mut state = None;
    for (key, value) in full_url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" => return Err(format!("Google sign-in was denied or failed: {}", value)),
            _ => {}
        }
    }

    let response_body = CALLBACK_PAGE_BODY;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response_body.len(),
        response_body
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;

    match (code, state) {
        (Some(code), Some(state)) if state == expected_state => Ok(code),
        (Some(_), Some(_)) => Err("OAuth state mismatch — possible CSRF, aborting".to_string()),
        _ => Err("Google sign-in callback was missing the authorization code".to_string()),
    }
}

pub struct ExchangedTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
}

pub async fn exchange_code(code: &str, verifier: &str, port: u16) -> Result<ExchangedTokens, String> {
    let client = reqwest::Client::new();
    let params = [
        ("code", code),
        ("client_id", client_id()?),
        ("client_secret", client_secret()?),
        ("redirect_uri", &redirect_uri(port)),
        ("grant_type", "authorization_code"),
        ("code_verifier", verifier),
    ];

    let response = client
        .post(TOKEN_ENDPOINT)
        .form(&params)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Failed to reach Google token endpoint: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Google token exchange failed ({}): {}", status, body));
    }

    let token: GoogleTokenResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Google token response: {}", e))?;

    let refresh_token = token.refresh_token.ok_or_else(|| {
        "Google did not return a refresh token — try disconnecting and reconnecting".to_string()
    })?;

    Ok(ExchangedTokens {
        access_token: token.access_token,
        refresh_token,
        expires_in: token.expires_in,
    })
}

pub struct RefreshedTokens {
    pub access_token: String,
    pub expires_in: i64,
}

pub async fn refresh_access_token(refresh_token: &str) -> Result<RefreshedTokens, String> {
    let client = reqwest::Client::new();
    let params = [
        ("refresh_token", refresh_token),
        ("client_id", client_id()?),
        ("client_secret", client_secret()?),
        ("grant_type", "refresh_token"),
    ];

    let response = client
        .post(TOKEN_ENDPOINT)
        .form(&params)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Failed to reach Google token endpoint: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Google token refresh failed ({}): {}", status, body));
    }

    let token: GoogleTokenResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Google token refresh response: {}", e))?;

    Ok(RefreshedTokens {
        access_token: token.access_token,
        expires_in: token.expires_in,
    })
}

pub async fn fetch_connected_email(access_token: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    let response = client
        .get(USERINFO_ENDPOINT)
        .bearer_auth(access_token)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch connected Google account info: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to fetch connected Google account info: HTTP {}",
            response.status()
        ));
    }

    let info: GoogleUserInfo = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Google userinfo response: {}", e))?;
    Ok(info.email)
}
