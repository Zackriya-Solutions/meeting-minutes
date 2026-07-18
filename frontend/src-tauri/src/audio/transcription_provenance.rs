use crate::state::AppState;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static METADATA_WRITE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranscriptionProvenance {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub language: Option<String>,
    pub transcribed_at: Option<String>,
    pub source: Option<String>,
    pub known: bool,
}

pub fn write_transcription_provenance(
    folder: &Path,
    provider: &str,
    model: &str,
    language: Option<&str>,
    source: &str,
) -> Result<()> {
    let _guard = METADATA_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let metadata_path = folder.join("metadata.json");
    let mut metadata = if metadata_path.exists() {
        let raw = std::fs::read_to_string(&metadata_path)
            .with_context(|| format!("Failed to read {}", metadata_path.display()))?;
        serde_json::from_str::<Value>(&raw)
            .with_context(|| format!("Failed to parse {}", metadata_path.display()))?
    } else {
        Value::Object(Map::new())
    };
    let object = metadata
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("metadata.json root must be an object"))?;
    let now = chrono::Utc::now().to_rfc3339();
    object.insert(
        "transcription".to_string(),
        serde_json::json!({
            "provider": provider,
            "model": model,
            "language": language.unwrap_or("auto"),
            "transcribed_at": now,
            "source": source,
        }),
    );

    let temporary = folder.join(format!(
        ".metadata.json.transcription-{}.tmp",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(&temporary, serde_json::to_string_pretty(&metadata)?)
        .with_context(|| format!("Failed to write {}", temporary.display()))?;
    std::fs::rename(&temporary, &metadata_path).with_context(|| {
        format!(
            "Failed to replace {} with {}",
            metadata_path.display(),
            temporary.display()
        )
    })?;
    Ok(())
}

pub fn read_transcription_provenance(folder: &Path) -> Result<TranscriptionProvenance> {
    let metadata_path = folder.join("metadata.json");
    if !metadata_path.exists() {
        return Ok(TranscriptionProvenance::default());
    }
    let raw = std::fs::read_to_string(&metadata_path)
        .with_context(|| format!("Failed to read {}", metadata_path.display()))?;
    let metadata: Value = serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse {}", metadata_path.display()))?;
    let nested = metadata.get("transcription").and_then(Value::as_object);
    let string = |key: &str| {
        nested
            .and_then(|value| value.get(key))
            .or_else(|| metadata.get(format!("transcription_{key}")))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let provider = string("provider");
    let model = string("model");
    Ok(TranscriptionProvenance {
        known: provider.is_some() && model.is_some(),
        provider,
        model,
        language: string("language"),
        transcribed_at: nested
            .and_then(|value| value.get("transcribed_at"))
            .or_else(|| metadata.get("retranscribed_at"))
            .or_else(|| metadata.get("completed_at"))
            .and_then(Value::as_str)
            .map(str::to_string),
        source: string("source"),
    })
}

#[tauri::command]
pub async fn get_meeting_transcription_provenance(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<TranscriptionProvenance, String> {
    let folder: Option<String> = sqlx::query_scalar("SELECT folder_path FROM meetings WHERE id=?")
        .bind(&meeting_id)
        .fetch_optional(state.db_manager.pool())
        .await
        .map_err(|error| format!("Database error: {error}"))?
        .flatten();
    let folder = folder
        .map(PathBuf::from)
        .ok_or_else(|| "Recording folder path is not available for this meeting".to_string())?;
    if !folder.is_dir() {
        return Err("Recording folder was not found".to_string());
    }
    read_transcription_provenance(&folder).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_provenance_without_losing_existing_metadata() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(
            folder.path().join("metadata.json"),
            r#"{"meeting_name":"Weekly sync","custom":true}"#,
        )
        .unwrap();
        write_transcription_provenance(
            folder.path(),
            "salutespeech",
            "salutespeech-stream-v2",
            Some("ru"),
            "retranscription",
        )
        .unwrap();

        let parsed = read_transcription_provenance(folder.path()).unwrap();
        assert!(parsed.known);
        assert_eq!(parsed.provider.as_deref(), Some("salutespeech"));
        assert_eq!(parsed.model.as_deref(), Some("salutespeech-stream-v2"));
        let raw = std::fs::read_to_string(folder.path().join("metadata.json")).unwrap();
        assert!(raw.contains("Weekly sync"));
        assert!(raw.contains("\"custom\": true"));
    }

    #[test]
    fn old_metadata_is_reported_as_unknown() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("metadata.json"), "{}").unwrap();
        assert!(!read_transcription_provenance(folder.path()).unwrap().known);
    }
}
