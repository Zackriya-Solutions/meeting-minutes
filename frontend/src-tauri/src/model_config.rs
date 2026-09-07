use serde::Deserialize;
use std::{collections::HashMap, fs, path::PathBuf};
use tauri::{AppHandle, Manager, Runtime};

pub const PROVIDER_MODELS_CONFIG: &str = "config/provider-models.json";
const DEFAULT_PROVIDER_MODELS_CONFIG: &str = include_str!("../config/provider-models.json");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfiguredModel {
    pub id: String,
    pub display_name: Option<String>,
    pub owned_by: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProviderModelConfig {
    providers: HashMap<String, ProviderModels>,
}

#[derive(Debug, Deserialize)]
struct ProviderModels {
    models: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ModelEntry {
    Id(String),
    Details(ModelDetails),
}

#[derive(Debug, Deserialize)]
struct ModelDetails {
    id: String,
    #[serde(rename = "displayName", alias = "display_name")]
    display_name: Option<String>,
    owned_by: Option<String>,
}

impl From<ModelEntry> for ConfiguredModel {
    fn from(entry: ModelEntry) -> Self {
        match entry {
            ModelEntry::Id(id) => Self {
                id,
                display_name: None,
                owned_by: None,
            },
            ModelEntry::Details(details) => Self {
                id: details.id,
                display_name: details.display_name,
                owned_by: details.owned_by,
            },
        }
    }
}

pub fn load_provider_models<R: Runtime>(app: Option<&AppHandle<R>>, provider: &str) -> Option<Vec<ConfiguredModel>> {
    load_provider_models_from_paths(provider, config_paths(app))
        .or_else(|| parse_provider_models(provider, DEFAULT_PROVIDER_MODELS_CONFIG))
}

fn config_paths<R: Runtime>(app: Option<&AppHandle<R>>) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(app) = app {
        if let Ok(resource_dir) = app.path().resource_dir() {
            paths.push(resource_dir.join(PROVIDER_MODELS_CONFIG));
        }
    }

    if let Ok(current_dir) = std::env::current_dir() {
        paths.push(current_dir.join(PROVIDER_MODELS_CONFIG));
        paths.push(current_dir.join("frontend/src-tauri").join(PROVIDER_MODELS_CONFIG));
    }

    paths
}

fn load_provider_models_from_paths(provider: &str, paths: Vec<PathBuf>) -> Option<Vec<ConfiguredModel>> {
    paths
        .into_iter()
        .find_map(|path| fs::read_to_string(path).ok())
        .and_then(|json| parse_provider_models(provider, &json))
}

fn parse_provider_models(provider: &str, json: &str) -> Option<Vec<ConfiguredModel>> {
    let config: ProviderModelConfig = serde_json::from_str(json).ok()?;
    let models: Vec<_> = config
        .providers
        .get(provider)?
        .models
        .iter()
        .filter_map(|entry| match entry {
            ModelEntry::Id(id) if !id.trim().is_empty() => Some(ConfiguredModel {
                id: id.trim().to_string(),
                display_name: None,
                owned_by: None,
            }),
            ModelEntry::Details(details) if !details.id.trim().is_empty() => Some(ConfiguredModel {
                id: details.id.trim().to_string(),
                display_name: details.display_name.clone(),
                owned_by: details.owned_by.clone(),
            }),
            _ => None,
        })
        .collect();

    if models.is_empty() {
        None
    } else {
        Some(models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_string_and_object_model_entries() {
        let json = r#"
        {
          "providers": {
            "openai": { "models": ["gpt-5.5"] },
            "anthropic": {
              "models": [
                { "id": "claude-test", "displayName": "Claude Test" },
                { "id": "claude_alias", "display_name": "Claude Alias" }
              ]
            }
          }
        }
        "#;

        let openai = parse_provider_models("openai", json).unwrap();
        assert_eq!(openai[0].id, "gpt-5.5");

        let anthropic = parse_provider_models("anthropic", json).unwrap();
        assert_eq!(anthropic[0].display_name.as_deref(), Some("Claude Test"));
        assert_eq!(anthropic[1].display_name.as_deref(), Some("Claude Alias"));
    }

    #[test]
    fn invalid_missing_or_empty_config_falls_back_to_none() {
        assert!(parse_provider_models("openai", "not json").is_none());
        assert!(parse_provider_models("missing", r#"{ "providers": {} }"#).is_none());
        assert!(parse_provider_models("openai", r#"{ "providers": { "openai": { "models": [] } } }"#).is_none());
    }

    #[test]
    fn bundled_default_config_contains_provider_models() {
        assert!(parse_provider_models("openai", DEFAULT_PROVIDER_MODELS_CONFIG)
            .unwrap()
            .iter()
            .any(|model| model.id == "gpt-5"));
        assert!(parse_provider_models("anthropic", DEFAULT_PROVIDER_MODELS_CONFIG)
            .unwrap()
            .iter()
            .any(|model| model.display_name.as_deref() == Some("Claude 4.5 Sonnet")));
        assert_eq!(
            parse_provider_models("groq", DEFAULT_PROVIDER_MODELS_CONFIG).unwrap()[0].id,
            "llama-3.3-70b-versatile"
        );
    }
}
