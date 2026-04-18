use tray_icon::menu::{Menu, MenuId, MenuItem, PredefinedMenuItem};

#[derive(Debug, Clone)]
pub struct MenuIds {
    pub open: MenuId,
    pub refresh: MenuId,
    pub quit: MenuId,
}

pub fn build() -> Result<(Menu, MenuIds), String> {
    let menu = Menu::new();
    let open = MenuItem::new("Open", true, None);
    let refresh = MenuItem::new("Refresh", true, None);
    let separator = PredefinedMenuItem::separator();
    let quit = MenuItem::new("Quit", true, None);

    menu.append_items(&[&open, &refresh, &separator, &quit])
        .map_err(|error| format!("Failed to build tray menu: {error}"))?;

    Ok((
        menu,
        MenuIds {
            open: open.id().clone(),
            refresh: refresh.id().clone(),
            quit: quit.id().clone(),
        },
    ))
}
