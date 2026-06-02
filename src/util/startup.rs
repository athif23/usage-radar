use std::fs;
use std::path::PathBuf;

const STARTUP_FILE_NAME: &str = "Usage Radar.cmd";

pub fn set_launch_at_startup(enabled: bool) -> Result<(), String> {
    if enabled {
        install_startup_launcher()
    } else {
        remove_startup_launcher()
    }
}

#[cfg(target_os = "windows")]
fn install_startup_launcher() -> Result<(), String> {
    let startup_file = startup_file_path()?;
    let parent = startup_file.parent().ok_or_else(|| {
        format!(
            "Startup file has no parent directory: {}",
            startup_file.display()
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "Failed to create startup directory {}: {error}",
            parent.display()
        )
    })?;

    let exe = std::env::current_exe()
        .map_err(|error| format!("Failed to determine the current executable path: {error}"))?;
    let script = format!(
        "@echo off\r\nstart \"\" \"{}\"\r\n",
        exe.display().to_string().replace('"', "\"\"")
    );

    fs::write(&startup_file, script).map_err(|error| {
        format!(
            "Failed to write startup launcher {}: {error}",
            startup_file.display()
        )
    })
}

#[cfg(not(target_os = "windows"))]
fn install_startup_launcher() -> Result<(), String> {
    Err("Launch at startup is currently only supported on Windows.".to_string())
}

fn remove_startup_launcher() -> Result<(), String> {
    let startup_file = startup_file_path()?;
    match fs::remove_file(&startup_file) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Failed to remove startup launcher {}: {error}",
            startup_file.display()
        )),
    }
}

#[cfg(target_os = "windows")]
fn startup_file_path() -> Result<PathBuf, String> {
    startup_dir().map(|path| path.join(STARTUP_FILE_NAME))
}

#[cfg(not(target_os = "windows"))]
fn startup_file_path() -> Result<PathBuf, String> {
    Err("Launch at startup is currently only supported on Windows.".to_string())
}

#[cfg(target_os = "windows")]
fn startup_dir() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|path| {
            path.join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Startup")
        })
        .ok_or_else(|| "Could not determine the Windows startup folder".to_string())
}
