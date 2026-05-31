use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::providers::ProviderKind;

const REFRESH_INTERVALS: [u64; 4] = [0, 1, 5, 15];
const TRACKABLE_PROVIDERS: [ProviderKind; 3] = [
    ProviderKind::Codex,
    ProviderKind::Copilot,
    ProviderKind::OpenCodeGo,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub selected_provider: Option<ProviderKind>,
    pub refresh_minutes: u64,
    pub start_in_tray: bool,
    #[serde(default)]
    pub sort_home_by_urgency: bool,
    #[serde(default)]
    pub disabled_providers: Vec<ProviderKind>,
    pub opencode_go_cookie_header: Option<String>,
    pub opencode_go_workspace_id: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            selected_provider: None,
            refresh_minutes: 5,
            start_in_tray: true,
            sort_home_by_urgency: false,
            disabled_providers: Vec::new(),
            opencode_go_cookie_header: None,
            opencode_go_workspace_id: None,
        }
    }
}

impl AppConfig {
    fn normalize(&mut self) {
        if !REFRESH_INTERVALS.contains(&self.refresh_minutes) {
            self.refresh_minutes = Self::default().refresh_minutes;
        }

        self.disabled_providers
            .retain(|provider| TRACKABLE_PROVIDERS.contains(provider));
        self.disabled_providers
            .sort_by_key(|provider| match provider {
                ProviderKind::Codex => 0,
                ProviderKind::Copilot => 1,
                ProviderKind::OpenCodeGo => 2,
                ProviderKind::ClaudeCode => 3,
            });
        self.disabled_providers.dedup();
    }
}

pub fn load(path: &Path) -> Result<Option<AppConfig>, String> {
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read config file {}: {error}", path.display()))?;
    let mut config: AppConfig = serde_json::from_str(&contents)
        .map_err(|error| format!("Failed to parse config file {}: {error}", path.display()))?;
    config.normalize();

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
