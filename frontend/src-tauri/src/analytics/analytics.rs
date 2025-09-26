use posthog_rs::{Client, Event};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use chrono::{DateTime, Utc, Duration};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsConfig {
    pub api_key: String,
    pub host: Option<String>,
    pub enabled: bool,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            host: Some("https://us.i.posthog.com".to_string()),
            enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub session_id: String,
    pub user_id: String,
    pub start_time: DateTime<Utc>,
    pub is_active: bool,
}

impl UserSession {
    pub fn new(user_id: String) -> Self {
        let now = Utc::now();
        Self {
            session_id: format!("session_{}", Uuid::new_v4()),
            user_id,
            start_time: now,
            is_active: true,
        }
    }

    pub fn duration_seconds(&self) -> i64 {
        (Utc::now() - self.start_time).num_seconds()
    }
}

pub struct AnalyticsClient {
    client: Option<Arc<Client>>,
    config: AnalyticsConfig,
    user_id: Arc<Mutex<Option<String>>>,
    current_session: Arc<Mutex<Option<UserSession>>>,
}

impl AnalyticsClient {
    pub async fn new(config: AnalyticsConfig) -> Self {
        let client = if config.enabled && !config.api_key.is_empty() {
            Some(Arc::new(posthog_rs::client(config.api_key.as_str()).await))
        } else {
            None
        };

        Self {
            client,
            config,
            user_id: Arc::new(Mutex::new(None)),
            current_session: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn identify(&self, user_id: String, properties: Option<HashMap<String, String>>) -> Result<(), String> {
        let client = match &self.client {
            Some(client) => Arc::clone(client),
            None => return Ok(()),
        };

        // Store user ID for future events
        *self.user_id.lock().await = Some(user_id.clone());

        let properties = properties.unwrap_or_default();
        
        let mut event = Event::new("$identify", &user_id);
        
        // Add user properties
        for (key, value) in properties {
            if let Err(e) = event.insert_prop(&key, value) {
                eprintln!("Failed to add property {}: {}", key, e);
            }
        }
        
        if let Err(e) = client.capture(event).await {
            eprintln!("Failed to identify user: {}", e);
        }
        
        Ok(())
    }

    pub async fn track_event(&self, event_name: &str, properties: Option<HashMap<String, String>>) -> Result<(), String> {
        let client = match &self.client {
            Some(client) => Arc::clone(client),
            None => return Ok(()),
        };

        let user_id = match self.user_id.lock().await.clone() {
            Some(id) => id,
            None => {
                // Don't create anonymous users, wait for proper identification
                eprintln!("Warning: Attempted to track event '{}' before user identification", event_name);
                return Ok(());
            }
        };

        let event_name = event_name.to_string();
        let mut properties = properties.unwrap_or_default();
        
        // Add session information to all events
        if let Some(session) = self.current_session.lock().await.as_ref() {
            properties.insert("session_id".to_string(), session.session_id.clone());
            properties.insert("session_duration".to_string(), session.duration_seconds().to_string());
        }
        
        let mut event = Event::new(&event_name, &user_id);
        
        // Add event properties
        for (key, value) in properties {
            if let Err(e) = event.insert_prop(&key, value) {
                eprintln!("Failed to add property {}: {}", key, e);
            }
        }
        
        if let Err(e) = client.capture(event).await {
            eprintln!("Failed to track event {}: {}", event_name, e);
        }
        
        Ok(())
    }

    // Enhanced user tracking methods
    pub async fn start_session(&self, user_id: String) -> Result<String, String> {
        let session = UserSession::new(user_id.clone());
        let session_id = session.session_id.clone();
        
        *self.current_session.lock().await = Some(session);
        
        let mut properties = HashMap::new();
        properties.insert("session_id".to_string(), session_id.clone());
        properties.insert("timestamp".to_string(), Utc::now().to_rfc3339());
        
        self.track_event("session_started", Some(properties)).await?;
        
        Ok(session_id)
    }

    pub async fn end_session(&self) -> Result<(), String> {
        let mut session_guard = self.current_session.lock().await;
        
        if let Some(session) = session_guard.take() {
            let mut properties = HashMap::new();
            properties.insert("session_id".to_string(), session.session_id.clone());
            properties.insert("session_duration".to_string(), session.duration_seconds().to_string());
            properties.insert("timestamp".to_string(), Utc::now().to_rfc3339());
            
            self.track_event("session_ended", Some(properties)).await?;
        }
        
        Ok(())
    }



    pub async fn track_daily_active_user(&self) -> Result<(), String> {
        let user_id = match self.user_id.lock().await.clone() {
            Some(id) => id,
            None => {
                eprintln!("Warning: Attempted to track daily active user before user identification");
                return Ok(());
            }
        };
        
        let mut properties = HashMap::new();
        properties.insert("user_id".to_string(), user_id);
        properties.insert("date".to_string(), Utc::now().format("%Y-%m-%d").to_string());
        properties.insert("timestamp".to_string(), Utc::now().to_rfc3339());
        
        self.track_event("daily_active_user", Some(properties)).await
    }

    pub async fn track_user_first_launch(&self) -> Result<(), String> {
        let mut properties = HashMap::new();
        properties.insert("timestamp".to_string(), Utc::now().to_rfc3339());
        properties.insert("app_version".to_string(), env!("CARGO_PKG_VERSION").to_string());
        
        self.track_event("user_first_launch", Some(properties)).await
    }

    pub async fn get_current_session(&self) -> Option<UserSession> {
        self.current_session.lock().await.clone()
    }

    pub async fn is_session_active(&self) -> bool {
        self.current_session.lock().await.is_some()
    }

    // Meeting-specific event tracking methods
    pub async fn track_meeting_started(&self, meeting_id: &str, meeting_title: &str) -> Result<(), String> {
        let mut properties = HashMap::new();
        properties.insert("meeting_id".to_string(), meeting_id.to_string());
        properties.insert("meeting_title".to_string(), meeting_title.to_string());
        properties.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());
        
        self.track_event("meeting_started", Some(properties)).await
    }

    pub async fn track_recording_started(&self, meeting_id: &str) -> Result<(), String> {
        let mut properties = HashMap::new();
        properties.insert("meeting_id".to_string(), meeting_id.to_string());
        properties.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());
        
        self.track_event("recording_started", Some(properties)).await
    }

    pub async fn track_recording_stopped(&self, meeting_id: &str, duration_seconds: Option<u64>) -> Result<(), String> {
        let mut properties = HashMap::new();
        properties.insert("meeting_id".to_string(), meeting_id.to_string());
        properties.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());
        
        if let Some(duration) = duration_seconds {
            properties.insert("duration_seconds".to_string(), duration.to_string());
        }
        
        self.track_event("recording_stopped", Some(properties)).await
    }

    pub async fn track_meeting_deleted(&self, meeting_id: &str) -> Result<(), String> {
        let mut properties = HashMap::new();
        properties.insert("meeting_id".to_string(), meeting_id.to_string());
        properties.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());
        
        self.track_event("meeting_deleted", Some(properties)).await
    }

    pub async fn track_search_performed(&self, query: &str, results_count: usize) -> Result<(), String> {
        let mut properties = HashMap::new();
        properties.insert("query".to_string(), query.to_string());
        properties.insert("results_count".to_string(), results_count.to_string());
        properties.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());
        
        self.track_event("search_performed", Some(properties)).await
    }

    pub async fn track_settings_changed(&self, setting_type: &str, new_value: &str) -> Result<(), String> {
        let mut properties = HashMap::new();
        properties.insert("setting_type".to_string(), setting_type.to_string());
        properties.insert("new_value".to_string(), new_value.to_string());
        properties.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());
        
        self.track_event("settings_changed", Some(properties)).await
    }

    pub async fn track_app_started(&self, version: &str) -> Result<(), String> {
        let mut properties = HashMap::new();
        properties.insert("app_version".to_string(), version.to_string());
        properties.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());
        
        self.track_event("app_started", Some(properties)).await
    }

    pub async fn track_feature_used(&self, feature_name: &str) -> Result<(), String> {
        let mut properties = HashMap::new();
        properties.insert("feature_name".to_string(), feature_name.to_string());
        properties.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());
        
        self.track_event("feature_used", Some(properties)).await
    }

    // Summary generation analytics
    pub async fn track_summary_generation_started(&self, model_provider: &str, model_name: &str, transcript_length: usize) -> Result<(), String> {
        let mut properties = HashMap::new();
        properties.insert("model_provider".to_string(), model_provider.to_string());
        properties.insert("model_name".to_string(), model_name.to_string());
        properties.insert("transcript_length".to_string(), transcript_length.to_string());
        properties.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());
        
        self.track_event("summary_generation_started", Some(properties)).await
    }

    pub async fn track_summary_generation_completed(&self, model_provider: &str, model_name: &str, success: bool, duration_seconds: Option<u64>, error_message: Option<&str>) -> Result<(), String> {
        let mut properties = HashMap::new();
        properties.insert("model_provider".to_string(), model_provider.to_string());
        properties.insert("model_name".to_string(), model_name.to_string());
        properties.insert("success".to_string(), success.to_string());
        properties.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());
        
        if let Some(duration) = duration_seconds {
            properties.insert("duration_seconds".to_string(), duration.to_string());
        }
        
        if let Some(error) = error_message {
            properties.insert("error_message".to_string(), error.to_string());
        }
        
        self.track_event("summary_generation_completed", Some(properties)).await
    }

    pub async fn track_summary_regenerated(&self, model_provider: &str, model_name: &str) -> Result<(), String> {
        let mut properties = HashMap::new();
        properties.insert("model_provider".to_string(), model_provider.to_string());
        properties.insert("model_name".to_string(), model_name.to_string());
        properties.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());
        
        self.track_event("summary_regenerated", Some(properties)).await
    }

    pub async fn track_model_changed(&self, old_provider: &str, old_model: &str, new_provider: &str, new_model: &str) -> Result<(), String> {
        let mut properties = HashMap::new();
        properties.insert("old_provider".to_string(), old_provider.to_string());
        properties.insert("old_model".to_string(), old_model.to_string());
        properties.insert("new_provider".to_string(), new_provider.to_string());
        properties.insert("new_model".to_string(), new_model.to_string());
        properties.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());
        
        self.track_event("model_changed", Some(properties)).await
    }

    pub async fn track_custom_prompt_used(&self, prompt_length: usize) -> Result<(), String> {
        let mut properties = HashMap::new();
        properties.insert("prompt_length".to_string(), prompt_length.to_string());
        properties.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());
        
        self.track_event("custom_prompt_used", Some(properties)).await
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled && self.client.is_some()
    }

    pub async fn set_user_properties(&self, properties: HashMap<String, String>) -> Result<(), String> {
        let client = match &self.client {
            Some(client) => Arc::clone(client),
            None => return Ok(()),
        };

        let user_id = match self.user_id.lock().await.clone() {
            Some(id) => id,
            None => {
                eprintln!("Warning: Attempted to set user properties before user identification");
                return Ok(());
            }
        };
        
        let mut event = Event::new("$set", &user_id);
        
        // Add user properties
        for (key, value) in properties {
            if let Err(e) = event.insert_prop(&key, value) {
                eprintln!("Failed to add property {}: {}", key, e);
            }
        }
        
        if let Err(e) = client.capture(event).await {
            eprintln!("Failed to set user properties: {}", e);
        }
        
        Ok(())
    }
}

// Helper function to create analytics client from config
pub async fn create_analytics_client(config: AnalyticsConfig) -> AnalyticsClient {
    AnalyticsClient::new(config).await
} 