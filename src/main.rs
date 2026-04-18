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
        .run_with(App::boot)
}
