pub mod message;
mod startup;
pub mod state;

use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Element, Length, Subscription, Task, Theme};

use self::message::Message;
use self::startup::load_startup;
pub use self::state::App;
use self::state::{FileLoadState, StartupReport};

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        (Self::from_startup(load_startup()), Task::none())
    }

    pub fn theme(&self) -> Theme {
        Theme::Dark
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::none()
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ReloadLocalState => {
                self.apply_startup(load_startup());
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let header = column![
            text("Usage Radar").size(32),
            text("Slice 0 scaffold").size(18),
            text("Baseline Iced shell with local config/cache loading.")
        ]
        .spacing(4);

        let controls = row![button(text("Reload local state")).on_press(Message::ReloadLocalState)]
            .spacing(12)
            .align_y(Alignment::Center);

        let content = column![
            header,
            controls,
            status_block(
                "Config",
                vec![
                    format!("State: {}", file_state_label(self.startup.config_state)),
                    format!("Path: {}", path_label(self.startup.config_path.as_ref())),
                    format!("Refresh minutes: {}", self.config.refresh_minutes),
                    format!("Start in tray: {}", yes_no(self.config.start_in_tray)),
                    format!(
                        "Selected provider: {}",
                        provider_label(self.config.selected_provider)
                    ),
                ],
            ),
            status_block(
                "Cache",
                vec![
                    format!("State: {}", file_state_label(self.startup.cache_state)),
                    format!("Path: {}", path_label(self.startup.cache_path.as_ref())),
                    format!("Schema version: {}", self.cache.version),
                    format!("Cached providers: {}", self.cache.providers.len()),
                ],
            ),
            status_block("Startup notes", startup_notes(&self.startup)),
        ]
        .spacing(16)
        .max_width(760);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(24)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }
}

fn status_block<'a>(title: &'a str, lines: Vec<String>) -> Element<'a, Message> {
    let mut body = column![text(title).size(20)].spacing(6);

    for line in lines {
        body = body.push(text(line));
    }

    container(body).width(Length::Fill).padding(16).into()
}

fn startup_notes(report: &StartupReport) -> Vec<String> {
    if report.notes.is_empty() {
        vec!["No startup warnings.".to_string()]
    } else {
        report.notes.clone()
    }
}

fn file_state_label(state: FileLoadState) -> &'static str {
    match state {
        FileLoadState::Loaded => "Loaded from disk",
        FileLoadState::Missing => "Missing file, using defaults",
        FileLoadState::Defaulted => "Error, using defaults",
        FileLoadState::NotChecked => "Not checked",
    }
}

fn path_label(path: Option<&std::path::PathBuf>) -> String {
    match path {
        Some(path) => path.display().to_string(),
        None => "Unavailable".to_string(),
    }
}

fn provider_label(provider: Option<crate::providers::ProviderKind>) -> &'static str {
    match provider {
        Some(provider) => provider.label(),
        None => "None",
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "Yes"
    } else {
        "No"
    }
}
