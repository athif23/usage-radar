use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::providers::ProviderSnapshot;

const CACHE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedSnapshots {
    pub version: u32,
    pub providers: Vec<ProviderSnapshot>,
}

impl Default for CachedSnapshots {
    fn default() -> Self {
        Self {
            version: CACHE_VERSION,
            providers: Vec::new(),
        }
    }
}

pub fn load(path: &Path) -> Result<Option<CachedSnapshots>, String> {
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read cache file {}: {error}", path.display()))?;
    let cache = serde_json::from_str(&contents)
        .map_err(|error| format!("Failed to parse cache file {}: {error}", path.display()))?;

    Ok(Some(cache))
}

#[allow(dead_code)]
pub fn save(path: &Path, cache: &CachedSnapshots) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Cache file has no parent directory: {}", path.display()))?;

    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "Failed to create cache directory {}: {error}",
            parent.display()
        )
    })?;

    let contents = serde_json::to_string_pretty(cache)
        .map_err(|error| format!("Failed to encode cache: {error}"))?;

    fs::write(path, contents)
        .map_err(|error| format!("Failed to write cache file {}: {error}", path.display()))
}
