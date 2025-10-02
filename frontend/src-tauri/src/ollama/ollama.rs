use std::process::Command;
use serde::{Deserialize, Serialize};
use tauri::command;
use reqwest::blocking::Client;

#[derive(Debug, Serialize, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    pub id: String,
    pub size: String,
    pub modified: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaApiResponse {
    models: Vec<OllamaApiModel>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaApiModel {
    name: String,
    model: String,
    modified_at: String,
    size: i64,
}

#[command]
pub fn get_ollama_models() -> Result<Vec<OllamaModel>, String> {
    // First try the HTTP API
    match get_models_via_http() {
        Ok(models) => Ok(models),
        Err(http_err) => {
            // Fallback to CLI if HTTP fails
            get_models_via_cli().map_err(|cli_err| {
                format!("HTTP API error: {}\nCLI error: {}", http_err, cli_err)
            })
        }
    }
}

fn get_models_via_http() -> Result<Vec<OllamaModel>, String> {
    let client = Client::new();
    let response = client
        .get("http://localhost:11434/api/tags")
        .send()
        .map_err(|e| format!("Failed to make HTTP request: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP request failed with status: {}", response.status()));
    }

    let api_response: OllamaApiResponse = response
        .json()
        .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

    Ok(api_response.models.into_iter().map(|m| OllamaModel {
        name: m.name,
        id: m.model,
        size: format_size(m.size),
        modified: m.modified_at,
    }).collect())
}

fn get_models_via_cli() -> Result<Vec<OllamaModel>, String> {
    let output = Command::new("ollama")
        .arg("list")
        .output()
        .map_err(|e| format!("Failed to execute ollama command: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    let mut models = Vec::new();
    
    // Skip the header line
    for line in output_str.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            models.push(OllamaModel {
                name: parts[0].to_string(),
                id: parts[1].to_string(),
                size: format!("{} {}", parts[2], parts[3]),
                modified: parts[4..].join(" "),
            });
        }
    }

    Ok(models)
}

fn format_size(size: i64) -> String {
    if size < 1024 {
        format!("{} B", size)
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else if size < 1024 * 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
