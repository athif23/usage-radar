use crate::app::state::{FileLoadState, StartupData, StartupReport};
use crate::storage::{cache, config};
use crate::util::paths;

pub fn load_startup() -> StartupData {
    let mut report = StartupReport::default();

    let config = match paths::config_file_path() {
        Ok(path) => {
            report.config_path = Some(path.clone());

            match config::load(&path) {
                Ok(Some(config)) => {
                    report.config_state = FileLoadState::Loaded;
                    config
                }
                Ok(None) => {
                    report.config_state = FileLoadState::Missing;
                    config::AppConfig::default()
                }
                Err(error) => {
                    report.config_state = FileLoadState::Defaulted;
                    report.notes.push(error);
                    config::AppConfig::default()
                }
            }
        }
        Err(error) => {
            report.config_state = FileLoadState::Defaulted;
            report.notes.push(error);
            config::AppConfig::default()
        }
    };

    let cache = match paths::cache_file_path() {
        Ok(path) => {
            report.cache_path = Some(path.clone());

            match cache::load(&path) {
                Ok(Some(cache)) => {
                    report.cache_state = FileLoadState::Loaded;
                    cache
                }
                Ok(None) => {
                    report.cache_state = FileLoadState::Missing;
                    cache::CachedSnapshots::default()
                }
                Err(error) => {
                    report.cache_state = FileLoadState::Defaulted;
                    report.notes.push(error);
                    cache::CachedSnapshots::default()
                }
            }
        }
        Err(error) => {
            report.cache_state = FileLoadState::Defaulted;
            report.notes.push(error);
            cache::CachedSnapshots::default()
        }
    };

    StartupData {
        config,
        cache,
        report,
    }
}
