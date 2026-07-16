// VALUEOS: native transport for the ValueOS Agent flow. NEW file in our namespace (the
// only upstream edits are the `pub mod valueos;` + command registrations in lib.rs and the
// deps in Cargo.toml). Owns: OAuth2 Auth-Code+PKCE login (tauri-plugin-oauth loopback),
// OS-keychain token storage (keyring), token refresh, and ALL authenticated ValueOS HTTP
// (reqwest) — so tokens never reach the webview and no CSP change is needed.
//
// ⚠️ FIRST CUT — compiled/validated via the CI/desktop build (Rust can't be built in the
// authoring environment). Config values come from env (Terraform outputs); see FEATURE-flow.md.
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Runtime};

const KEYCHAIN_SERVICE: &str = "com.valueos.io";
const KEYCHAIN_USER: &str = "agent-tokens";

// ---- config (from env; real values are Terraform outputs cognito_agent_*) --------------
fn cfg_client_id() -> String { std::env::var("VALUEOS_CLIENT_ID").unwrap_or_default() }
fn cfg_hosted_ui() -> String { std::env::var("VALUEOS_HOSTED_UI_BASE").unwrap_or_default() }
fn cfg_api_base() -> String { std::env::var("VALUEOS_API_BASE").unwrap_or_default() }
fn cfg_ports() -> Vec<u16> { vec![8765, 14321] }
const SCOPES: &str =
    "valueos/read:tenants valueos/read:leads valueos/read:opportunities valueos/write:transcripts";

// ---- error envelope (serialized to the JS side; TS maps to ValueOsApiError) ------------
#[derive(Debug, Serialize)]
pub struct ValueOsErr {
    pub status: u16,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<serde_json::Value>,
}
impl ValueOsErr {
    fn new(status: u16, message: impl Into<String>) -> Self {
        Self { status, message: message.into(), scope: None, feature: None, fields: None }
    }
    fn transport(message: impl Into<String>) -> Self {
        Self::new(0, message) // status 0 → TS treats as retryable transport failure
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct StoredTokens {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_at: i64, // epoch ms
    #[serde(default)]
    scopes: Vec<String>,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

// percent-encode a query value using the already-present `url` crate (no extra dep)
fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

// ---- keychain ---------------------------------------------------------------------------
fn keychain_entry() -> Result<keyring::Entry, ValueOsErr> {
    keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_USER)
        .map_err(|e| ValueOsErr::transport(format!("keychain: {e}")))
}
fn save_tokens(t: &StoredTokens) -> Result<(), ValueOsErr> {
    let json = serde_json::to_string(t).map_err(|e| ValueOsErr::transport(e.to_string()))?;
    keychain_entry()?
        .set_password(&json)
        .map_err(|e| ValueOsErr::transport(format!("keychain set: {e}")))
}
fn read_tokens() -> Option<StoredTokens> {
    let entry = keychain_entry().ok()?;
    let json = entry.get_password().ok()?;
    serde_json::from_str(&json).ok()
}
fn clear_tokens() -> Result<(), ValueOsErr> {
    if let Ok(entry) = keychain_entry() {
        let _ = entry.delete_credential();
    }
    Ok(())
}

// ---- OAuth2 token exchange / refresh ----------------------------------------------------
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

async fn exchange_code(code: &str, verifier: &str, redirect_uri: &str) -> Result<StoredTokens, ValueOsErr> {
    let client = reqwest::Client::new();
    let params = [
        ("grant_type", "authorization_code"),
        ("client_id", &cfg_client_id()),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("code_verifier", verifier),
    ];
    let resp = client
        .post(format!("{}/oauth2/token", cfg_hosted_ui()))
        .form(&params)
        .send()
        .await
        .map_err(|e| ValueOsErr::transport(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(ValueOsErr::new(401, "Token exchange failed"));
    }
    let tr: TokenResponse = resp.json().await.map_err(|e| ValueOsErr::transport(e.to_string()))?;
    Ok(StoredTokens {
        access_token: tr.access_token,
        refresh_token: tr.refresh_token,
        expires_at: now_ms() + tr.expires_in.unwrap_or(3600) * 1000,
        scopes: SCOPES.split(' ').map(|s| s.to_string()).collect(),
    })
}

async fn refresh(tokens: &StoredTokens) -> Result<StoredTokens, ValueOsErr> {
    let refresh_token = tokens
        .refresh_token
        .clone()
        .ok_or_else(|| ValueOsErr::new(401, "No refresh token"))?;
    let client = reqwest::Client::new();
    let params = [
        ("grant_type", "refresh_token"),
        ("client_id", &cfg_client_id()),
        ("refresh_token", &refresh_token),
    ];
    let resp = client
        .post(format!("{}/oauth2/token", cfg_hosted_ui()))
        .form(&params)
        .send()
        .await
        .map_err(|e| ValueOsErr::transport(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(ValueOsErr::new(401, "Token refresh failed"));
    }
    let tr: TokenResponse = resp.json().await.map_err(|e| ValueOsErr::transport(e.to_string()))?;
    let updated = StoredTokens {
        access_token: tr.access_token,
        refresh_token: tr.refresh_token.or(tokens.refresh_token.clone()),
        expires_at: now_ms() + tr.expires_in.unwrap_or(3600) * 1000,
        scopes: tokens.scopes.clone(),
    };
    save_tokens(&updated)?;
    Ok(updated)
}

/// Returns a valid access token, refreshing if near expiry; 401 if not logged in / cannot refresh.
async fn valid_access_token() -> Result<String, ValueOsErr> {
    let tokens = read_tokens().ok_or_else(|| ValueOsErr::new(401, "Not logged in"))?;
    if tokens.expires_at - 30_000 > now_ms() {
        return Ok(tokens.access_token);
    }
    let refreshed = refresh(&tokens).await?;
    Ok(refreshed.access_token)
}

// ---- authenticated ValueOS HTTP ---------------------------------------------------------
async fn map_http_error(resp: reqwest::Response) -> ValueOsErr {
    let status = resp.status().as_u16();
    let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
    ValueOsErr {
        status,
        message: body
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Request failed")
            .to_string(),
        scope: body.get("scope").and_then(|v| v.as_str()).map(String::from),
        feature: body.get("feature").and_then(|v| v.as_str()).map(String::from),
        fields: body.get("fields").cloned(),
    }
}

async fn api_get(path: &str) -> Result<serde_json::Value, ValueOsErr> {
    let token = valid_access_token().await?;
    let resp = reqwest::Client::new()
        .get(format!("{}{}", cfg_api_base(), path))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| ValueOsErr::transport(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(map_http_error(resp).await);
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| ValueOsErr::transport(e.to_string()))?;
    Ok(body.get("result").cloned().unwrap_or(serde_json::Value::Null))
}

async fn api_post(path: &str, payload: &serde_json::Value) -> Result<serde_json::Value, ValueOsErr> {
    let token = valid_access_token().await?;
    let resp = reqwest::Client::new()
        .post(format!("{}{}", cfg_api_base(), path))
        .bearer_auth(token)
        .json(payload)
        .send()
        .await
        .map_err(|e| ValueOsErr::transport(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(map_http_error(resp).await);
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| ValueOsErr::transport(e.to_string()))?;
    Ok(body.get("result").cloned().unwrap_or(serde_json::Value::Null))
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

// ---- commands ---------------------------------------------------------------------------
#[tauri::command]
pub async fn valueos_is_logged_in() -> Result<bool, ValueOsErr> {
    Ok(read_tokens().map(|t| t.expires_at - 30_000 > now_ms() || t.refresh_token.is_some()).unwrap_or(false))
}

#[tauri::command]
pub async fn valueos_logout() -> Result<(), ValueOsErr> {
    clear_tokens()
}

#[tauri::command]
pub async fn valueos_login() -> Result<(), ValueOsErr> {
    // PKCE
    let mut verifier_bytes = [0u8; 32];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut verifier_bytes);
    let verifier = b64url(&verifier_bytes);
    let challenge = b64url(Sha256::digest(verifier.as_bytes()).as_slice());
    let mut state_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut state_bytes);
    let state = b64url(&state_bytes);

    // Loopback (tauri-plugin-oauth) — capture the redirect URL via a channel.
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let mut sender = Some(tx);
    let port = tauri_plugin_oauth::start_with_config(
        // Only pin the ports; let all other fields (redirect_uri, response, …) default —
        // resilient to this plugin version's exact OauthConfig shape.
        tauri_plugin_oauth::OauthConfig { ports: Some(cfg_ports()), ..Default::default() },
        move |url| {
            if let Some(tx) = sender.take() {
                let _ = tx.send(url);
            }
        },
    )
    .map_err(|e| ValueOsErr::transport(format!("loopback: {e}")))?;

    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    let authorize_url = format!(
        "{}/oauth2/authorize?response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}",
        cfg_hosted_ui(),
        enc(&cfg_client_id()),
        enc(&redirect_uri),
        enc(SCOPES),
        enc(&challenge),
        enc(&state),
    );
    open_browser(&authorize_url);

    // Wait up to 5 minutes for the browser redirect.
    let url = tokio::time::timeout(std::time::Duration::from_secs(300), rx)
        .await
        .map_err(|_| ValueOsErr::new(408, "Login timed out"))?
        .map_err(|_| ValueOsErr::transport("Login cancelled"))?;

    let parsed = url::Url::parse(&url).map_err(|e| ValueOsErr::transport(e.to_string()))?;
    let mut code: Option<String> = None;
    let mut got_state: Option<String> = None;
    for (k, v) in parsed.query_pairs() {
        match k.as_ref() {
            "code" => code = Some(v.into_owned()),
            "state" => got_state = Some(v.into_owned()),
            _ => {}
        }
    }
    if got_state.as_deref() != Some(state.as_str()) {
        return Err(ValueOsErr::new(401, "State mismatch"));
    }
    let code = code.ok_or_else(|| ValueOsErr::new(401, "No authorization code"))?;
    let tokens = exchange_code(&code, &verifier, &redirect_uri).await?;
    save_tokens(&tokens)?;
    Ok(())
}

#[tauri::command]
pub async fn valueos_api_get_tenants() -> Result<serde_json::Value, ValueOsErr> {
    api_get("/me/tenants").await
}

#[tauri::command]
pub async fn valueos_api_get_entitlement(tenant_id: String) -> Result<serde_json::Value, ValueOsErr> {
    api_get(&format!("/me/entitlements?tenant_id={}", enc(&tenant_id))).await
}

fn list_query(q: Option<String>, limit: Option<u32>, offset: Option<u32>) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(q) = q.filter(|s| !s.is_empty()) {
        parts.push(format!("q={}", enc(&q)));
    }
    if let Some(l) = limit {
        parts.push(format!("limit={l}"));
    }
    if let Some(o) = offset {
        parts.push(format!("offset={o}"));
    }
    if parts.is_empty() { String::new() } else { format!("?{}", parts.join("&")) }
}

#[tauri::command]
pub async fn valueos_api_list_leads(
    tenant_id: String,
    q: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<serde_json::Value, ValueOsErr> {
    api_get(&format!("/tenants/{tenant_id}/leads{}", list_query(q, limit, offset))).await
}

#[tauri::command]
pub async fn valueos_api_list_opportunities(
    tenant_id: String,
    q: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<serde_json::Value, ValueOsErr> {
    api_get(&format!("/tenants/{tenant_id}/opportunities{}", list_query(q, limit, offset))).await
}

#[tauri::command]
pub async fn valueos_api_upload_transcript(
    tenant_id: String,
    activity_type: String,
    target_id: String,
    request: serde_json::Value,
) -> Result<serde_json::Value, ValueOsErr> {
    let plural = if activity_type == "lead" { "leads" } else { "opportunities" };
    api_post(&format!("/tenants/{tenant_id}/{plural}/{target_id}/transcripts"), &request).await
}

/// High-level recap. FIRST CUT: deterministic extractive summary (readable prose, not a
/// hash). TODO(valueos): upgrade to reuse crate::summary::llm_client::generate_summary.
#[tauri::command]
pub async fn valueos_generate_digest(
    transcript: String,
    title: Option<String>,
    max_chars: Option<usize>,
) -> Result<String, ValueOsErr> {
    let clean = transcript.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.is_empty() {
        return Ok(format!(
            "{}No speech was captured.",
            title.map(|t| format!("{t} — ")).unwrap_or_default()
        ));
    }
    let words = clean.split(' ').count();
    let lead: String = clean
        .split_inclusive(['.', '!', '?'])
        .take(5)
        .collect::<String>()
        .trim()
        .to_string();
    let header = title.map(|t| format!("Recap — {t}")).unwrap_or_else(|| "Meeting recap".into());
    let out = format!("{header}\n\nOverview: a {words}-word conversation.\n\nKey points:\n{lead}");
    let max = max_chars.unwrap_or(4000);
    // char-safe truncation (never split a UTF-8 boundary)
    Ok(if out.chars().count() > max {
        format!("{}…", out.chars().take(max.saturating_sub(1)).collect::<String>())
    } else {
        out
    })
}

#[tauri::command]
pub async fn valueos_pick_folder<R: Runtime>(app: AppHandle<R>) -> Result<Option<String>, ValueOsErr> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app.dialog().file().blocking_pick_folder();
    Ok(picked.map(|p| p.to_string()))
}

#[tauri::command]
pub async fn valueos_validate_writable(path: String) -> Result<bool, ValueOsErr> {
    let dir = std::path::Path::new(&path);
    if !dir.is_dir() {
        return Ok(false);
    }
    let probe = dir.join(".valueos-write-test");
    match std::fs::write(&probe, b"ok") {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

#[tauri::command]
pub async fn valueos_write_transcript_file(
    folder: String,
    file_name: String,
    content: String,
) -> Result<String, ValueOsErr> {
    let path = std::path::Path::new(&folder).join(&file_name);
    std::fs::write(&path, content).map_err(|e| ValueOsErr::transport(e.to_string()))?;
    Ok(path.to_string_lossy().to_string())
}
