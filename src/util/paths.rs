use std::path::PathBuf;

const APP_DIR_NAME: &str = "UsageRadar";

pub fn config_dir() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|path| path.join(APP_DIR_NAME))
        .ok_or_else(|| "Could not determine the roaming config directory".to_string())
}

pub fn cache_dir() -> Result<PathBuf, String> {
    dirs::data_local_dir()
        .map(|path| path.join(APP_DIR_NAME))
        .ok_or_else(|| "Could not determine the local app data directory".to_string())
}

#[allow(dead_code)]
pub fn logs_dir() -> Result<PathBuf, String> {
    Ok(cache_dir()?.join("logs"))
}

pub fn config_file_path() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("config.json"))
}

pub fn cache_file_path() -> Result<PathBuf, String> {
    Ok(cache_dir()?.join("snapshots.json"))
}
