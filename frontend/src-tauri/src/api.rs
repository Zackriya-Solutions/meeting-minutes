use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;
use log::{info as log_info, error as log_error, debug as log_debug, warn as log_warn};

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Meeting {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TranscriptSearchResult {
    pub id: String,
    pub title: String,
    #[serde(rename = "matchContext")]
    pub match_context: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileRequest {
    pub email: String,
    pub license_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SaveProfileRequest {
    pub id: String,
    pub email: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateProfileRequest {
    pub email: String,
    pub license_key: String,
    pub company: String,
    pub position: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub model: String,
    #[serde(rename = "whisperModel")]
    pub whisper_model: String,
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SaveModelConfigRequest {
    pub provider: String,
    pub model: String,
    #[serde(rename = "whisperModel")]
    pub whisper_model: String,
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetApiKeyRequest {
    pub provider: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TranscriptConfig {
    pub provider: String,
    pub model: String,
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SaveTranscriptConfigRequest {
    pub provider: String,
    pub model: String,
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteMeetingRequest {
    pub meeting_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MeetingDetails {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub transcripts: Vec<MeetingTranscript>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MeetingTranscript {
    pub id: String,
    pub text: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SaveMeetingTitleRequest {
    pub meeting_id: String,
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SaveMeetingSummaryRequest {
    pub meeting_id: String,
    pub summary: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SummaryResponse {
    pub status: String,
    #[serde(rename = "meetingName")]
    pub meeting_name: Option<String>,
    pub meeting_id: String,
    pub start: Option<String>,
    pub end: Option<String>,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SaveTranscriptRequest {
    pub meeting_title: String,
    pub transcripts: Vec<TranscriptSegment>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub id: String,
    pub text: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessTranscriptRequest {
    pub text: String,
    pub model: String,
    pub model_name: String,
    pub meeting_id: Option<String>,
    pub chunk_size: Option<i32>,
    pub overlap: Option<i32>,
    pub custom_prompt: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessTranscriptResponse {
    pub message: String,
    pub process_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: Option<String>,
    pub email: String,
    pub license_key: String,
    pub company: Option<String>,
    pub position: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub is_licensed: bool,
}

// Helper function to get auth token from store (optional)
async fn get_auth_token<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    let store = match app.store("store.json") {
        Ok(store) => store,
        Err(_) => return None,
    };
    
    match store.get("authToken") {
        Some(token) => {
            if let Some(token_str) = token.as_str() {
                log_info!("Found auth token: {}", &token_str[..std::cmp::min(20, token_str.len())]);
                Some(token_str.to_string())
            } else {
                log_warn!("Auth token is not a string");
                None
            }
        }
        None => {
            log_warn!("No auth token found in store");
            None
        },
    }
}

// Helper function to get server address from store
async fn get_server_address<R: Runtime>(app: &AppHandle<R>) -> Result<String, String> {
    let store = app.store("store.json").map_err(|e| e.to_string())?;
    
    match store.get("appServerUrl") {
        Some(url) => {
            if let Some(url_str) = url.as_str() {
                log_info!("Using server URL: {}", url_str);
                Ok(url_str.to_string())
            } else {
                log_warn!("Server URL is not a string");
                Err("Server URL is not a string".to_string())
            }
        }
        None => {
            log_warn!("No server URL found, using default: http://localhost:5168");
            Ok("http://localhost:5168".to_string()) // Default fallback to match current backend port
        },
    }
}

// Generic API call function with optional authentication
async fn make_api_request<R: Runtime, T: for<'de> Deserialize<'de>>(
    app: &AppHandle<R>,
    endpoint: &str,
    method: &str,
    body: Option<&str>,
    additional_headers: Option<HashMap<String, String>>,
    auth_token: Option<String>, // Pass auth token from frontend
) -> Result<T, String> {
    let client = reqwest::Client::new();
    let server_url = get_server_address(app).await?;
    
    let url = format!("{}{}", server_url, endpoint);
    log_info!("Making {} request to: {}", method, url);
    
    let mut request = match method.to_uppercase().as_str() {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        _ => return Err(format!("Unsupported HTTP method: {}", method)),
    };
    
    // Add authorization header if auth token is provided
    if let Some(token) = auth_token {
        log_info!("Adding authorization header");
        request = request.header("Authorization", format!("Bearer {}", token));
    } else {
        log_warn!("No auth token provided, making unauthenticated request");
    }
    
    request = request.header("Content-Type", "application/json");
    
    // Add additional headers if provided
    if let Some(headers) = additional_headers {
        for (key, value) in headers {
            request = request.header(&key, &value);
        }
    }
    
    // Add body if provided
    if let Some(body_str) = body {
        request = request.body(body_str.to_string());
    }
    
    let response = request.send().await.map_err(|e| {
        let error_msg = format!("Request failed: {}", e);
        log_error!("{}", error_msg);
        error_msg
    })?;
    
    let status = response.status();
    log_info!("Response status: {}", status);
    
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        let error_msg = format!("HTTP {}: {}", status, error_text);
        log_error!("{}", error_msg);
        return Err(error_msg);
    }
    
    let response_text = response.text().await.map_err(|e| {
        let error_msg = format!("Failed to read response: {}", e);
        log_error!("{}", error_msg);
        error_msg
    })?;
    
    log_info!("Response body: {}", &response_text[..std::cmp::min(200, response_text.len())]);
    
    serde_json::from_str(&response_text).map_err(|e| {
        let error_msg = format!("Failed to parse JSON: {}", e);
        log_error!("{}", error_msg);
        error_msg
    })
}

// API Commands for Tauri

#[tauri::command]
pub async fn api_get_meetings<R: Runtime>(
    app: AppHandle<R>, 
    auth_token: Option<String>
) -> Result<Vec<Meeting>, String> {
    log_info!("api_get_meetings called with auth_token: {}", auth_token.is_some());
    
    let cache_headers = HashMap::from([
        ("Cache-Control".to_string(), "no-cache, no-store, must-revalidate".to_string()),
        ("Pragma".to_string(), "no-cache".to_string()),
        ("Expires".to_string(), "0".to_string()),
    ]);
    
    let result = make_api_request::<R, Vec<Meeting>>(&app, "/get-meetings", "GET", None, Some(cache_headers), auth_token).await;
    
    match &result {
        Ok(meetings) => log_info!("Successfully got {} meetings", meetings.len()),
        Err(e) => log_error!("Error getting meetings: {}", e),
    }
    
    result
}

#[tauri::command]
pub async fn api_search_transcripts<R: Runtime>(
    app: AppHandle<R>,
    query: String,
    auth_token: Option<String>,
) -> Result<Vec<TranscriptSearchResult>, String> {
    log_info!("api_search_transcripts called with query: {}, auth_token: {}", query, auth_token.is_some());
    
    let search_request = SearchRequest { query };
    let body = serde_json::to_string(&search_request).map_err(|e| e.to_string())?;
    
    make_api_request::<R, Vec<TranscriptSearchResult>>(&app, "/search-transcripts", "POST", Some(&body), None, auth_token).await
}

#[tauri::command]
pub async fn api_get_profile<R: Runtime>(
    app: AppHandle<R>,
    email: String,
    license_key: String,
    auth_token: Option<String>,
) -> Result<Profile, String> {
    log_info!("api_get_profile called for email: {}, auth_token: {}", email, auth_token.is_some());
    
    let profile_request = ProfileRequest { email, license_key };
    let body = serde_json::to_string(&profile_request).map_err(|e| e.to_string())?;
    
    make_api_request::<R, Profile>(&app, "/get-profile", "POST", Some(&body), None, auth_token).await
}

#[tauri::command]
pub async fn api_save_profile<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    email: String,
    auth_token: Option<String>,
) -> Result<serde_json::Value, String> {
    log_info!("api_save_profile called for email: {}, auth_token: {}", email, auth_token.is_some());
    
    let save_request = SaveProfileRequest { id, email };
    let body = serde_json::to_string(&save_request).map_err(|e| e.to_string())?;
    
    make_api_request::<R, serde_json::Value>(&app, "/save-profile", "POST", Some(&body), None, auth_token).await
}

#[tauri::command]
pub async fn api_update_profile<R: Runtime>(
    app: AppHandle<R>,
    email: String,
    license_key: String,
    company: String,
    position: String,
    auth_token: Option<String>,
) -> Result<serde_json::Value, String> {
    log_info!("api_update_profile called for email: {}, auth_token: {}", email, auth_token.is_some());
    
    let update_request = UpdateProfileRequest { 
        email, 
        license_key, 
        company, 
        position 
    };
    let body = serde_json::to_string(&update_request).map_err(|e| e.to_string())?;
    
    make_api_request::<R, serde_json::Value>(&app, "/update-profile", "POST", Some(&body), None, auth_token).await
}

#[tauri::command]
pub async fn api_get_model_config<R: Runtime>(
    app: AppHandle<R>,
    auth_token: Option<String>,
) -> Result<Option<ModelConfig>, String> {
    log_info!("api_get_model_config called with auth_token: {}", auth_token.is_some());
    
    make_api_request::<R, Option<ModelConfig>>(&app, "/get-model-config", "GET", None, None, auth_token).await
}

#[tauri::command]
pub async fn api_save_model_config<R: Runtime>(
    app: AppHandle<R>,
    provider: String,
    model: String,
    whisper_model: String,
    api_key: Option<String>,
    auth_token: Option<String>,
) -> Result<serde_json::Value, String> {
    log_info!("api_save_model_config called for provider: {}, auth_token: {}", provider, auth_token.is_some());
    
    let save_request = SaveModelConfigRequest { 
        provider, 
        model, 
        whisper_model, 
        api_key 
    };
    let body = serde_json::to_string(&save_request).map_err(|e| e.to_string())?;
    
    make_api_request::<R, serde_json::Value>(&app, "/save-model-config", "POST", Some(&body), None, auth_token).await
}

#[tauri::command]
pub async fn api_get_api_key<R: Runtime>(
    app: AppHandle<R>,
    provider: String,
    auth_token: Option<String>,
) -> Result<String, String> {
    log_info!("api_get_api_key called for provider: {}, auth_token: {}", provider, auth_token.is_some());
    
    let request = GetApiKeyRequest { provider };
    let body = serde_json::to_string(&request).map_err(|e| e.to_string())?;
    
    make_api_request::<R, String>(&app, "/get-api-key", "POST", Some(&body), None, auth_token).await
}

#[tauri::command]
pub async fn api_get_transcript_config<R: Runtime>(
    app: AppHandle<R>,
    auth_token: Option<String>,
) -> Result<Option<TranscriptConfig>, String> {
    log_info!("api_get_transcript_config called with auth_token: {}", auth_token.is_some());
    
    make_api_request::<R, Option<TranscriptConfig>>(&app, "/get-transcript-config", "GET", None, None, auth_token).await
}

#[tauri::command]
pub async fn api_save_transcript_config<R: Runtime>(
    app: AppHandle<R>,
    provider: String,
    model: String,
    api_key: Option<String>,
    auth_token: Option<String>,
) -> Result<serde_json::Value, String> {
    log_info!("api_save_transcript_config called for provider: {}, auth_token: {}", provider, auth_token.is_some());
    
    let save_request = SaveTranscriptConfigRequest { 
        provider, 
        model, 
        api_key 
    };
    let body = serde_json::to_string(&save_request).map_err(|e| e.to_string())?;
    
    make_api_request::<R, serde_json::Value>(&app, "/save-transcript-config", "POST", Some(&body), None, auth_token).await
}

#[tauri::command]
pub async fn api_get_transcript_api_key<R: Runtime>(
    app: AppHandle<R>,
    provider: String,
    auth_token: Option<String>,
) -> Result<String, String> {
    log_info!("api_get_transcript_api_key called for provider: {}, auth_token: {}", provider, auth_token.is_some());
    
    let request = GetApiKeyRequest { provider };
    let body = serde_json::to_string(&request).map_err(|e| e.to_string())?;
    
    make_api_request::<R, String>(&app, "/get-transcript-api-key", "POST", Some(&body), None, auth_token).await
}

#[tauri::command]
pub async fn api_delete_meeting<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    auth_token: Option<String>,
) -> Result<serde_json::Value, String> {
    log_info!("api_delete_meeting called for meeting_id: {}, auth_token: {}", meeting_id, auth_token.is_some());
    
    let delete_request = DeleteMeetingRequest { meeting_id };
    let body = serde_json::to_string(&delete_request).map_err(|e| e.to_string())?;
    
    make_api_request::<R, serde_json::Value>(&app, "/delete-meeting", "POST", Some(&body), None, auth_token).await
}

#[tauri::command]
pub async fn api_get_meeting<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    auth_token: Option<String>,
) -> Result<MeetingDetails, String> {
    log_info!("api_get_meeting called for meeting_id: {}, auth_token: {}", meeting_id, auth_token.is_some());
    
    make_api_request::<R, MeetingDetails>(&app, &format!("/get-meeting/{}", meeting_id), "GET", None, None, auth_token).await
}

#[tauri::command]
pub async fn api_save_meeting_title<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    title: String,
    auth_token: Option<String>,
) -> Result<serde_json::Value, String> {
    log_info!("api_save_meeting_title called for meeting_id: {}, auth_token: {}", meeting_id, auth_token.is_some());
    
    let save_request = SaveMeetingTitleRequest { meeting_id, title };
    let body = serde_json::to_string(&save_request).map_err(|e| e.to_string())?;
    
    make_api_request::<R, serde_json::Value>(&app, "/save-meeting-title", "POST", Some(&body), None, auth_token).await
}

#[tauri::command]
pub async fn api_save_meeting_summary<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    summary: serde_json::Value,
    auth_token: Option<String>,
) -> Result<serde_json::Value, String> {
    log_info!("api_save_meeting_summary called for meeting_id: {}, auth_token: {}", meeting_id, auth_token.is_some());
    
    let save_request = SaveMeetingSummaryRequest { meeting_id, summary };
    let body = serde_json::to_string(&save_request).map_err(|e| e.to_string())?;
    
    make_api_request::<R, serde_json::Value>(&app, "/save-meeting-summary", "POST", Some(&body), None, auth_token).await
}

#[tauri::command]
pub async fn api_get_summary<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    auth_token: Option<String>,
) -> Result<SummaryResponse, String> {
    log_debug!("=== api_get_summary DEBUG ===");
    log_debug!("meeting_id: {}", meeting_id);
    log_debug!("auth_token present: {}", auth_token.is_some());
    if let Some(ref token) = auth_token {
        log_debug!("auth_token length: {}", token.len());
    }
    
    let result = make_api_request::<R, SummaryResponse>(&app, &format!("/get-summary/{}", meeting_id), "GET", None, None, auth_token).await;
    
    match &result {
        Ok(summary) => log_debug!("✓ api_get_summary successful"),
        Err(e) => log_error!("✗ api_get_summary failed: {}", e),
    }
    
    result
}

#[tauri::command]
pub async fn api_save_transcript<R: Runtime>(
    app: AppHandle<R>,
    meeting_title: String,
    transcripts: Vec<serde_json::Value>,
    auth_token: Option<String>,
) -> Result<serde_json::Value, String> {
    log_info!("api_save_transcript called for meeting: {}, transcripts: {}, auth_token: {}", 
             meeting_title, transcripts.len(), auth_token.is_some());
    
    // Convert serde_json::Value to TranscriptSegment
    let transcript_segments: Result<Vec<TranscriptSegment>, _> = transcripts
        .into_iter()
        .map(|t| serde_json::from_value(t))
        .collect();
    
    let transcript_segments = transcript_segments.map_err(|e| e.to_string())?;
    
    let save_request = SaveTranscriptRequest { 
        meeting_title, 
        transcripts: transcript_segments 
    };
    let body = serde_json::to_string(&save_request).map_err(|e| e.to_string())?;
    
    make_api_request::<R, serde_json::Value>(&app, "/save-transcript", "POST", Some(&body), None, auth_token).await
}

#[tauri::command]
pub async fn api_process_transcript<R: Runtime>(
    app: AppHandle<R>,
    text: String,
    model: String,
    model_name: String,
    meeting_id: Option<String>,
    chunk_size: Option<i32>,
    overlap: Option<i32>,
    custom_prompt: Option<String>,
    auth_token: Option<String>,
) -> Result<ProcessTranscriptResponse, String> {
    log_info!("api_process_transcript called for meeting_id: {:?}, model: {}, auth_token: {}", 
             meeting_id, model, auth_token.is_some());
    
    let process_request = ProcessTranscriptRequest {
        text,
        model,
        model_name,
        meeting_id,
        chunk_size,
        overlap,
        custom_prompt,
    };
    let body = serde_json::to_string(&process_request).map_err(|e| e.to_string())?;
    
    make_api_request::<R, ProcessTranscriptResponse>(&app, "/process-transcript", "POST", Some(&body), None, auth_token).await
}

// Debug command to check store contents
#[tauri::command]
pub async fn debug_store_contents<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    let store = app.store("store.json").map_err(|e| e.to_string())?;
    
    let auth_token = store.get("authToken");
    let server_url = store.get("appServerUrl");
    let license_key = store.get("licenseKey");
    
    let debug_info = format!(
        "Store contents:\n- authToken: {}\n- appServerUrl: {}\n- licenseKey: {}",
        if auth_token.is_some() { "present" } else { "missing" },
        server_url.as_ref().and_then(|v| v.as_str()).unwrap_or("missing"),
        if license_key.is_some() { "present" } else { "missing" }
    );
    
    log_debug!("{}", debug_info);
    Ok(debug_info)
}

// Simple test command to check backend connectivity
#[tauri::command]
pub async fn test_backend_connection<R: Runtime>(
    app: AppHandle<R>,
    auth_token: Option<String>
) -> Result<String, String> {
    log_debug!("Testing backend connection...");
    
    let client = reqwest::Client::new();
    let server_url = get_server_address(&app).await?;
    
    log_debug!("Testing connection to: {}", server_url);
    
    let mut request = client.get(&format!("{}/docs", server_url));
    
    if let Some(token) = auth_token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }
    
    match request.send().await {
        Ok(response) => {
            let status = response.status();
            log_debug!("Backend responded with status: {}", status);
            Ok(format!("Backend is reachable. Status: {}", status))
        }
        Err(e) => {
            let error_msg = format!("Failed to connect to backend: {}", e);
            log_debug!("{}", error_msg);
            Err(error_msg)
        }
    }
} 

#[tauri::command]
pub async fn debug_backend_connection<R: Runtime>(
    app: AppHandle<R>,
) -> Result<String, String> {
    log_debug!("=== DEBUG: Testing backend connection ===");
    
    // Test 1: Check server address from store
    let server_url = match get_server_address(&app).await {
        Ok(url) => {
            log_debug!("✓ Server URL from store: {}", url);
            url
        }
        Err(e) => {
            log_error!("✗ Failed to get server URL: {}", e);
            return Err(format!("Failed to get server URL: {}", e));
        }
    };
    
    // Test 2: Make a simple HTTP request to the backend
    let client = reqwest::Client::new();
    let test_url = format!("{}/docs", server_url); // Try the docs endpoint which should be public
    
    log_debug!("Testing connection to: {}", test_url);
    
    match client.get(&test_url).send().await {
        Ok(response) => {
            let status = response.status();
            log_debug!("✓ Backend responded with status: {}", status);
            Ok(format!("Backend connection successful! Status: {}, URL: {}", status, server_url))
        }
        Err(e) => {
            log_error!("✗ Backend connection failed: {}", e);
            Err(format!("Backend connection failed: {}", e))
        }
    }
} 

#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), String> {
    use std::process::Command;
    
    let result = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(&["/C", "start", &url])
            .output()
    } else if cfg!(target_os = "macos") {
        Command::new("open")
            .arg(&url)
            .output()
    } else {
        // Linux and other Unix-like systems
        Command::new("xdg-open")
            .arg(&url)
            .output()
    };
    
    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Failed to open URL: {}", e))
    }
} 