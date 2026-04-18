use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::providers::ProviderKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub selected_provider: Option<ProviderKind>,
    pub refresh_minutes: u64,
    pub start_in_tray: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            selected_provider: None,
            refresh_minutes: 5,
            start_in_tray: true,
        }
    }
}

pub fn load(path: &Path) -> Result<Option<AppConfig>, String> {
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read config file {}: {error}", path.display()))?;
    let config = serde_json::from_str(&contents)
        .map_err(|error| format!("Failed to parse config file {}: {error}", path.display()))?;

    Ok(Some(config))
}

#[allow(dead_code)]
pub fn save(path: &Path, config: &AppConfig) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Config file has no parent directory: {}", path.display()))?;

    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "Failed to create config directory {}: {error}",
            parent.display()
        )
    })?;

    let contents = serde_json::to_string_pretty(config)
        .map_err(|error| format!("Failed to encode config: {error}"))?;

    fs::write(path, contents)
        .map_err(|error| format!("Failed to write config file {}: {error}", path.display()))
}
