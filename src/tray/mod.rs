mod icon;
mod menu;

use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconId};

pub use menu::MenuIds;

#[derive(Default)]
pub struct State {
    pub icon: Option<TrayIcon>,
    pub id: Option<TrayIconId>,
    pub menu_ids: Option<MenuIds>,
    pub init_error: Option<String>,
}

impl State {
    pub fn is_ready(&self) -> bool {
        self.icon.is_some() && self.menu_ids.is_some()
    }

    pub fn clear_error(&mut self) {
        self.init_error = None;
    }
}

pub fn build() -> Result<(TrayIcon, MenuIds), String> {
    let (menu, ids) = menu::build()?;
    let icon = icon::build()?;

    let tray_icon = TrayIconBuilder::new()
        .with_id("usage-radar")
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("Usage Radar")
        .with_icon(icon)
        .build()
        .map_err(|error| format!("Failed to create tray icon: {error}"))?;

    Ok((tray_icon, ids))
}
