use std::path::Path;

/// Embeds the Google Calendar OAuth client id/secret as compile-time env vars,
/// read from a local, gitignored credentials file (never committed).
///
/// The file is the standard "Desktop app" JSON download from Google Cloud
/// Console (Credentials -> OAuth 2.0 Client IDs), saved at
/// `frontend/src-tauri/secrets/google_oauth_client.json`.
///
/// If the file is absent (e.g. a contributor's machine without calendar
/// integration configured), the env vars are simply not set; `calendar::oauth`
/// handles that at runtime with a clear "not configured" error rather than
/// failing the build.
pub fn embed_credentials_if_present() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let secrets_path = Path::new(&manifest_dir)
        .join("secrets")
        .join("google_oauth_client.json");

    println!("cargo:rerun-if-changed={}", secrets_path.display());

    let raw = match std::fs::read_to_string(&secrets_path) {
        Ok(raw) => raw,
        Err(_) => {
            println!(
                "cargo:warning=Google Calendar OAuth credentials not found at {} — calendar integration will be disabled until they're added",
                secrets_path.display()
            );
            return;
        }
    };

    let parsed: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            println!(
                "cargo:warning=Failed to parse Google OAuth credentials JSON: {}",
                e
            );
            return;
        }
    };

    // Google's downloaded JSON nests fields under "installed" for Desktop app clients.
    let creds = parsed.get("installed").unwrap_or(&parsed);

    let client_id = creds.get("client_id").and_then(|v| v.as_str());
    let client_secret = creds.get("client_secret").and_then(|v| v.as_str());

    match (client_id, client_secret) {
        (Some(id), Some(secret)) => {
            println!("cargo:rustc-env=GOOGLE_CALENDAR_CLIENT_ID={}", id);
            println!("cargo:rustc-env=GOOGLE_CALENDAR_CLIENT_SECRET={}", secret);
        }
        _ => {
            println!("cargo:warning=Google OAuth credentials file is missing client_id/client_secret");
        }
    }
}
