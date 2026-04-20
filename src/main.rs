#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod panel;
mod providers;
mod storage;
mod tray;
mod util;

use app::App;

fn main() -> iced::Result {
    iced::daemon(App::title, App::update, App::view)
        .theme(App::theme)
        .subscription(App::subscription)
        .font(lucide_icons::LUCIDE_FONT_BYTES)
        .run_with(App::boot)
}
