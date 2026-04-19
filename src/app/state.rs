use std::path::PathBuf;
use std::time::SystemTime;

use crate::panel;
use crate::providers::ProviderKind;
use crate::storage::cache::CachedSnapshots;
use crate::storage::config::AppConfig;
use crate::tray;

pub struct App {
    pub config: AppConfig,
    pub cache: CachedSnapshots,
    pub startup: StartupReport,
    pub tray: tray::State,
    pub panel: panel::State,
    pub refresh: RefreshState,
    pub runtime_notice: Option<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshReason {
    Startup,
    PanelOpened,
    Manual,
    Interval,
}

#[derive(Debug, Clone, Default)]
pub struct RefreshState {
    pub in_flight: bool,
    pub queued_reason: Option<RefreshReason>,
    pub last_reason: Option<RefreshReason>,
    pub last_started_at: Option<SystemTime>,
    pub last_finished_at: Option<SystemTime>,
    pub last_error: Option<String>,
}

impl App {
    pub fn from_startup(data: StartupData) -> Self {
        let mut panel = panel::State::default();
        panel.selected_provider = data.config.selected_provider.unwrap_or(ProviderKind::Codex);

        Self {
            config: data.config,
            cache: data.cache,
            startup: data.report,
            tray: tray::State::default(),
            panel,
            refresh: RefreshState::default(),
            runtime_notice: None,
        }
    }
}
