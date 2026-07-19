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
use tauri::{AppHandle, Manager, Runtime};

const KEYCHAIN_SERVICE: &str = "com.valueos.io";
const KEYCHAIN_USER: &str = "agent-tokens";

// ---- config (Terraform outputs cognito_agent_*; PUBLIC — public client, no secret) ------
// Baked as defaults so the packaged desktop app has them (it can't read shell env); still
// overridable via env for other deployments.
fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).ok().filter(|s| !s.is_empty()).unwrap_or_else(|| default.to_string())
}
fn cfg_client_id() -> String {
    env_or("VALUEOS_CLIENT_ID", "3kjnt13ct6k25u2hkvqatkfrrm")
}
fn cfg_hosted_ui() -> String {
    env_or(
        "VALUEOS_HOSTED_UI_BASE",
        "https://va-pptx-agents-agent.auth.eu-central-2.amazoncognito.com",
    )
}
fn cfg_api_base() -> String {
    env_or("VALUEOS_API_BASE", "https://d2luofz0a4v7f3.cloudfront.net/api/agent/v1")
}
fn cfg_ports() -> Vec<u16> { vec![8765, 14321] }
// VALUEOS_AGENT_API.md §1: standard `openid` + the ValueOS agent scopes (incl. write:bug-reports,
// required by POST /bug-reports — must be in the AUTHORIZE request, not merely allowed on the
// client, or the endpoint returns 403 {scope}).
const SCOPES: &str =
    "openid valueos/read:tenants valueos/read:leads valueos/read:opportunities valueos/write:transcripts valueos/read:releases valueos/write:telemetry valueos/write:bug-reports";

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

// Extract a human-readable detail from an OAuth2/Cognito error body, which is JSON like
// {"error":"invalid_grant","error_description":"…"}. Falls back to a trimmed snippet.
fn oauth_error_detail(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        let e = v.get("error").and_then(|x| x.as_str());
        let d = v.get("error_description").and_then(|x| x.as_str());
        match (e, d) {
            (Some(e), Some(d)) => return format!(": {e} — {d}"),
            (Some(e), None) => return format!(": {e}"),
            _ => {}
        }
    }
    let t = body.trim();
    if t.is_empty() { String::new() } else { format!(": {}", t.chars().take(200).collect::<String>()) }
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
    let client = http_client(HTTP_GET_TIMEOUT_SECS);
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
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(ValueOsErr::new(401, format!("Token exchange failed ({status}){}", oauth_error_detail(&body))));
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
    let client = http_client(HTTP_GET_TIMEOUT_SECS);
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
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(ValueOsErr::new(401, format!("Token refresh failed ({status}){}", oauth_error_detail(&body))));
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

/// Force a token refresh regardless of expiry — used to recover from a server 401
/// mid-session (contract §2.6). Returns the new access token, or an auth error.
async fn force_refresh() -> Result<String, ValueOsErr> {
    let tokens = read_tokens().ok_or_else(|| ValueOsErr::new(401, "Not logged in"))?;
    Ok(refresh(&tokens).await?.access_token)
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

// VALUEOS: a reqwest client with a hard total-request timeout so a stalled ValueOS call can
// never hang the app forever (e.g. an upload that stops making progress). On the resulting
// transport error the pending-upload queue keeps the local copy and lets the user retry — far
// better than an infinite "Uploading…". Falls back to a default client if the builder fails.
fn http_client(timeout_secs: u64) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}
const HTTP_GET_TIMEOUT_SECS: u64 = 30;
const HTTP_POST_TIMEOUT_SECS: u64 = 90; // uploads (transcript + digest) may be larger

async fn api_get(path: &str) -> Result<serde_json::Value, ValueOsErr> {
    let url = format!("{}{}", cfg_api_base(), path);
    let token = valid_access_token().await?;
    let mut resp = http_client(HTTP_GET_TIMEOUT_SECS)
        .get(url.as_str())
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| ValueOsErr::transport(e.to_string()))?;
    // Contract §2.6: on 401, force one token refresh and retry the call once.
    if resp.status().as_u16() == 401 {
        let token = force_refresh().await?;
        resp = http_client(HTTP_GET_TIMEOUT_SECS)
            .get(url.as_str())
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| ValueOsErr::transport(e.to_string()))?;
    }
    if !resp.status().is_success() {
        return Err(map_http_error(resp).await);
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| ValueOsErr::transport(e.to_string()))?;
    Ok(body.get("result").cloned().unwrap_or(serde_json::Value::Null))
}

async fn api_post(path: &str, payload: &serde_json::Value) -> Result<serde_json::Value, ValueOsErr> {
    let url = format!("{}{}", cfg_api_base(), path);
    let token = valid_access_token().await?;
    let mut resp = http_client(HTTP_POST_TIMEOUT_SECS)
        .post(url.as_str())
        .bearer_auth(token)
        .json(payload)
        .send()
        .await
        .map_err(|e| ValueOsErr::transport(e.to_string()))?;
    // Contract §2.6: on 401, force one token refresh and retry once (idempotency_key makes
    // the retried upload safe — the server replays the same ids, never duplicates).
    if resp.status().as_u16() == 401 {
        let token = force_refresh().await?;
        resp = http_client(HTTP_POST_TIMEOUT_SECS)
            .post(url.as_str())
            .bearer_auth(token)
            .json(payload)
            .send()
            .await
            .map_err(|e| ValueOsErr::transport(e.to_string()))?;
    }
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
    Ok(read_tokens()
        .map(|t| {
            // A token minted before a scope was added won't carry it, and a refresh keeps the
            // original grant — only a fresh authorize adds it. So a stored token whose scopes don't
            // cover the current SCOPES set is treated as NOT logged in, forcing one clean re-login
            // after an update that widened the scopes (else the resumed session would 403 on the
            // new scope).
            let has_all_scopes = SCOPES.split(' ').all(|s| t.scopes.iter().any(|have| have == s));
            let alive = t.expires_at - 30_000 > now_ms() || t.refresh_token.is_some();
            has_all_scopes && alive
        })
        .unwrap_or(false))
}

#[tauri::command]
pub async fn valueos_logout() -> Result<(), ValueOsErr> {
    clear_tokens()
}

/// DIAGNOSTIC: decode and return only the CLAIMS (payload) of the stored access token so a
/// 401 from the API can be triaged (client_id / token_use / scope / iss / exp). Deliberately
/// returns ONLY these claims — never the signature or the raw token — so nothing usable as a
/// credential is exposed. Returns null if not logged in.
#[tauri::command]
pub async fn valueos_debug_token_claims() -> Result<serde_json::Value, ValueOsErr> {
    let tokens = match read_tokens() {
        Some(t) => t,
        None => return Ok(serde_json::Value::Null),
    };
    // A JWT is header.payload.signature — decode ONLY the middle (payload) segment.
    let payload_b64 = tokens
        .access_token
        .split('.')
        .nth(1)
        .ok_or_else(|| ValueOsErr::new(0, "Access token is not a JWT"))?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64.as_bytes())
        .map_err(|e| ValueOsErr::transport(format!("claims decode: {e}")))?;
    let claims: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| ValueOsErr::transport(format!("claims parse: {e}")))?;
    let pick = |k: &str| claims.get(k).cloned().unwrap_or(serde_json::Value::Null);
    // Only the fields useful for diagnosing a rejected token — not the whole payload.
    Ok(serde_json::json!({
        "client_id": pick("client_id"),
        "token_use": pick("token_use"),
        "scope": pick("scope"),
        "iss": pick("iss"),
        "exp": pick("exp"),
        "sub": pick("sub"),
        "username": pick("username"),
    }))
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
    let redirect = tokio::time::timeout(std::time::Duration::from_secs(300), rx).await;
    // Release the loopback listener so its port is free for a later login THIS session
    // (otherwise repeated logout→login cycles would exhaust the pinned ports). Safe on all
    // outcomes — success, timeout, or a dropped channel.
    let _ = tauri_plugin_oauth::cancel(port);
    let url = redirect
        .map_err(|_| ValueOsErr::new(408, "Login timed out"))?
        .map_err(|_| ValueOsErr::transport("Login cancelled"))?;

    let parsed = url::Url::parse(&url).map_err(|e| ValueOsErr::transport(e.to_string()))?;
    let mut code: Option<String> = None;
    let mut got_state: Option<String> = None;
    let mut oauth_error: Option<String> = None;
    let mut oauth_error_desc: Option<String> = None;
    for (k, v) in parsed.query_pairs() {
        match k.as_ref() {
            "code" => code = Some(v.into_owned()),
            "state" => got_state = Some(v.into_owned()),
            "error" => oauth_error = Some(v.into_owned()),
            "error_description" => oauth_error_desc = Some(v.into_owned()),
            _ => {}
        }
    }
    // If the IdP redirected back with an error (access_denied, invalid_scope, …), report it
    // verbatim rather than a generic "no code".
    if let Some(err) = oauth_error {
        let desc = oauth_error_desc.unwrap_or_default();
        let msg = if desc.is_empty() {
            format!("Sign-in failed: {err}")
        } else {
            format!("Sign-in failed: {err} — {desc}")
        };
        return Err(ValueOsErr::new(401, msg));
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

/// The post-login gate (contract §2): tenants where the agent add-on is ACTIVE right now,
/// plus total_memberships. The webview uses this to gate the app; not-a-member / never /
/// expired tenants are filtered server-side.
#[tauri::command]
pub async fn valueos_api_get_agent_tenants() -> Result<serde_json::Value, ValueOsErr> {
    api_get("/me/agent-tenants").await
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

/// PRIMARY write path: create a call activity AND attach its transcript+digest in one atomic
/// op. The lead/opportunity link (XOR) is in the `request` body, not the path.
#[tauri::command]
pub async fn valueos_api_create_call(
    tenant_id: String,
    request: serde_json::Value,
) -> Result<serde_json::Value, ValueOsErr> {
    api_post(&format!("/tenants/{tenant_id}/calls"), &request).await
}

/// Fallback write path: attach a transcript to an existing lead/opportunity (link in the path).
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

/// VALUEOS WS3: save a scrubbed bug-report bundle to the app data dir (interim — until the
/// ValueOS bug-report endpoint exists). Returns the file path. Content is already scrubbed
/// (no tokens/PII/transcript text) by the webview before it reaches here.
#[tauri::command]
pub async fn valueos_save_bug_report<R: Runtime>(app: AppHandle<R>, content: String) -> Result<String, ValueOsErr> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| ValueOsErr::transport(format!("app_data_dir: {e}")))?
        .join("bug-reports");
    std::fs::create_dir_all(&dir).map_err(|e| ValueOsErr::transport(format!("mkdir: {e}")))?;
    let path = dir.join(format!("report-{}.json", new_uuid_v4()));
    std::fs::write(&path, content).map_err(|e| ValueOsErr::transport(format!("write bug report: {e}")))?;
    Ok(path.to_string_lossy().to_string())
}

/// Delete a local transcript file (best-effort). A missing file is NOT an error — the user
/// just wants it gone. Only touches the local file; never the ValueOS cloud copy.
#[tauri::command]
pub async fn valueos_delete_file(path: String) -> Result<(), ValueOsErr> {
    match std::fs::remove_file(&path) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(ValueOsErr::transport(format!("delete file: {e}"))),
    }
}

// ---- WS4: updater + telemetry -----------------------------------------------------------
// Prompt-first, notify-only. The webview never sees the token: check + telemetry go through
// the same authenticated api_get/api_post as every other ValueOS call, and the presigned
// installer is downloaded + checksum-verified natively before the user's confirmed apply.

// A UUIDv4 string built from the already-present `rand` (no new dependency).
fn new_uuid_v4() -> String {
    use rand::RngCore;
    let mut b = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40; // version 4
    b[8] = (b[8] & 0x3f) | 0x80; // RFC 4122 variant
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    )
}

fn open_path(path: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", path]).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

/// Stable, agent-generated install id, persisted in the app data dir and generated ONCE.
/// Survives updates (app data is not touched by an install).
#[tauri::command]
pub async fn valueos_install_id<R: Runtime>(app: AppHandle<R>) -> Result<String, ValueOsErr> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| ValueOsErr::transport(format!("app_data_dir: {e}")))?;
    let path = dir.join("valueos-install-id");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    std::fs::create_dir_all(&dir).map_err(|e| ValueOsErr::transport(format!("mkdir: {e}")))?;
    let id = new_uuid_v4();
    std::fs::write(&path, &id).map_err(|e| ValueOsErr::transport(format!("write install id: {e}")))?;
    Ok(id)
}

/// Platform + current app version, for the updates/check query and telemetry.
#[tauri::command]
pub async fn valueos_app_info<R: Runtime>(app: AppHandle<R>) -> Result<serde_json::Value, ValueOsErr> {
    Ok(serde_json::json!({
        "platform": std::env::consts::OS, // "macos" | "windows" | "linux"
        "version": app.package_info().version.to_string(),
    }))
}

/// GET /tenants/{tid}/updates/check?platform=&current_version= (read:releases + feat_agent).
#[tauri::command]
pub async fn valueos_api_check_update(
    tenant_id: String,
    platform: String,
    current_version: String,
) -> Result<serde_json::Value, ValueOsErr> {
    let path = format!(
        "/tenants/{}/updates/check?platform={}&current_version={}",
        enc(&tenant_id),
        enc(&platform),
        enc(&current_version)
    );
    api_get(&path).await
}

/// POST /tenants/{tid}/telemetry (write:telemetry + feat_agent). `event` is the JSON body.
#[tauri::command]
pub async fn valueos_api_report_telemetry(
    tenant_id: String,
    event: serde_json::Value,
) -> Result<(), ValueOsErr> {
    let path = format!("/tenants/{}/telemetry", enc(&tenant_id));
    api_post(&path, &event).await?;
    Ok(())
}

/// POST /bug-reports (scope write:bug-reports; NOT feat_agent-gated, tenant optional — reporting
/// works even if the tenant's Agent add-on lapsed). ValueOS creates the GitHub issue server-side;
/// the agent never holds the GitHub token. `report` is the JSON body (`description` required;
/// title/version/route/userAgent/platform/context/logs/tenant_name/screenshot optional). Returns
/// `{ issue_number, issue_url }`; the API's server-side redaction runs on logs/context/title.
#[tauri::command]
pub async fn valueos_api_report_bug(report: serde_json::Value) -> Result<serde_json::Value, ValueOsErr> {
    api_post("/bug-reports", &report).await
}

/// Download the presigned installer and VERIFY its checksum (when provided) BEFORE it can be
/// applied. Returns the staged file path. No auth header — the URL is already presigned.
#[tauri::command]
pub async fn valueos_download_update(
    download_url: String,
    expected_sha256: Option<String>,
) -> Result<String, ValueOsErr> {
    let resp = reqwest::Client::new()
        .get(&download_url)
        .send()
        .await
        .map_err(|e| ValueOsErr::transport(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(ValueOsErr::new(
            resp.status().as_u16(),
            format!("update download failed: HTTP {}", resp.status().as_u16()),
        ));
    }
    let bytes = resp.bytes().await.map_err(|e| ValueOsErr::transport(e.to_string()))?;
    let data: &[u8] = &bytes;
    if let Some(expected) = expected_sha256.filter(|s| !s.trim().is_empty()) {
        // Hex-encode the digest manually (GenericArray has no LowerHex; avoids a hex dep).
        let actual: String = Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect();
        if !actual.eq_ignore_ascii_case(expected.trim()) {
            return Err(ValueOsErr::new(0, "checksum mismatch — update rejected".to_string()));
        }
    }
    let fname = download_url
        .split('?')
        .next()
        .and_then(|p| p.rsplit('/').next())
        .filter(|s| !s.is_empty())
        .unwrap_or("valueos-agent-update")
        .to_string();
    let dir = std::env::temp_dir().join("valueos-updates");
    std::fs::create_dir_all(&dir).map_err(|e| ValueOsErr::transport(format!("mkdir: {e}")))?;
    let path = dir.join(fname);
    std::fs::write(&path, data).map_err(|e| ValueOsErr::transport(format!("write update: {e}")))?;
    Ok(path.to_string_lossy().to_string())
}

/// Apply a downloaded+verified installer: hand it to the OS and exit so it can replace the app
/// (the user relaunches the new version). We NEVER auto-install — this only runs after the user
/// confirmed. No user data is touched.
#[tauri::command]
pub async fn valueos_apply_update<R: Runtime>(app: AppHandle<R>, path: String) -> Result<(), ValueOsErr> {
    open_path(&path);
    app.exit(0);
    Ok(())
}
