use std::path::PathBuf;

use crate::storage::cache::CachedSnapshots;
use crate::storage::config::AppConfig;

pub struct App {
    pub config: AppConfig,
    pub cache: CachedSnapshots,
    pub startup: StartupReport,
}

#[derive(Debug, Clone)]
pub struct StartupData {
    pub config: AppConfig,
    pub cache: CachedSnapshots,
    pub report: StartupReport,
}

#[derive(Debug, Clone, Default)]
pub struct StartupReport {
    pub config_path: Option<PathBuf>,
    pub cache_path: Option<PathBuf>,
    pub config_state: FileLoadState,
    pub cache_state: FileLoadState,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum FileLoadState {
    Loaded,
    Missing,
    Defaulted,
    #[default]
    NotChecked,
}

impl App {
    pub fn from_startup(data: StartupData) -> Self {
        Self {
            config: data.config,
            cache: data.cache,
            startup: data.report,
        }
    }

    pub fn apply_startup(&mut self, data: StartupData) {
        self.config = data.config;
        self.cache = data.cache;
        self.startup = data.report;
    }
}
