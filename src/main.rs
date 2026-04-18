mod app;
mod providers;
mod storage;
mod util;

use app::App;

fn main() -> iced::Result {
    iced::application("Usage Radar", App::update, App::view)
        .theme(App::theme)
        .subscription(App::subscription)
        .run_with(App::boot)
}
