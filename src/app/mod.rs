pub mod message;
mod startup;
pub mod state;

use std::fs;
use std::process::Command;
use std::time::{Duration, SystemTime};

use iced::widget::{button, column, container, progress_bar, row, scrollable, text};
use iced::{
    event, keyboard, window, Alignment, Border, Color, Element, Event, Length, Shadow,
    Subscription, Task, Theme,
};
use tray_icon::menu::MenuEvent;
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

use crate::providers::{ProviderKind, ProviderSnapshot};
use crate::storage::{cache as cache_store, config as config_store};
use crate::util::paths;

use self::message::Message;
use self::startup::load_startup;
pub use self::state::App;
use self::state::{FileLoadState, RefreshReason};

const STALE_GRACE: Duration = Duration::from_secs(30 * 60);

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        (
            Self::from_startup(load_startup()),
            Task::done(Message::AppStarted),
        )
    }

    pub fn title(&self, _window: window::Id) -> String {
        "Usage Radar".to_string()
    }

    pub fn theme(&self, _window: window::Id) -> Theme {
        Theme::Dark
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            iced::time::every(Duration::from_millis(75)).map(|_| Message::Tick),
            window::close_requests().map(Message::PanelCloseRequested),
            window::close_events().map(Message::PanelClosed),
        ];

        if self.config.refresh_minutes > 0 {
            subscriptions.push(
                iced::time::every(Duration::from_secs(self.config.refresh_minutes * 60))
                    .map(|_| Message::RefreshRequested(RefreshReason::Interval)),
            );
        }

        if self.panel.visible {
            subscriptions.push(event::listen_with(handle_escape_key));
        }

        Subscription::batch(subscriptions)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AppStarted => self.handle_app_started(),
            Message::Tick => self.poll_shell_events(),
            Message::PanelScrolled => {
                self.panel.note_scrolled();
                Task::none()
            }
            Message::SelectProvider(kind) => self.handle_select_provider(kind),
            Message::OpenConfigFolder => self.open_config_folder(),
            Message::RefreshRequested(reason) => self.request_refresh(reason),
            Message::RefreshFinished(result) => self.handle_refresh_finished(result),
            Message::HidePanel => self.hide_panel(),
            Message::EscapePressed(id) => {
                if self.panel.id == Some(id) && self.panel.visible {
                    self.hide_panel()
                } else {
                    Task::none()
                }
            }
            Message::PanelOpened(id) => self.handle_panel_opened(id),
            Message::PanelScaleFactorLoaded(id, scale_factor) => {
                if self.panel.id == Some(id) {
                    self.panel.scale_factor = scale_factor;
                }

                Task::none()
            }
            Message::PanelCloseRequested(id) => {
                if self.panel.id == Some(id) {
                    self.hide_panel()
                } else {
                    Task::none()
                }
            }
            Message::PanelClosed(id) => {
                if self.panel.id == Some(id) {
                    self.panel.id = None;
                    self.panel.visible = false;
                }

                Task::none()
            }
        }
    }

    pub fn view(&self, window_id: window::Id) -> Element<'_, Message> {
        if self.panel.id != Some(window_id) {
            return container(text("")).into();
        }

        let mut content = column![self.top_bar_view(), self.selected_provider_view()].spacing(10);

        if let Some(notice) = self.notice_text() {
            content = content.push(notice_view(notice, self.notice_tone()));
        }

        let scrollable_content = scrollable(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .direction(iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::new()
                    .width(3)
                    .scroller_width(4)
                    .margin(1),
            ))
            .on_scroll(|_| Message::PanelScrolled)
            .style(move |_theme, status| {
                panel_scrollable_style(status, self.panel.scrollbar_is_active())
            });

        let layout = column![scrollable_content, self.bottom_bar_view()]
            .spacing(12)
            .height(Length::Fill);

        container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(14)
            .style(panel_shell_style)
            .into()
    }

    fn handle_app_started(&mut self) -> Task<Message> {
        let tray_ready = self.initialize_tray();
        let start_visible = !self.config.start_in_tray || !tray_ready;
        let skip_taskbar = tray_ready && self.config.start_in_tray;

        self.open_panel_window(start_visible, skip_taskbar, None)
            .chain(Task::done(Message::RefreshRequested(
                RefreshReason::Startup,
            )))
    }

    fn initialize_tray(&mut self) -> bool {
        if self.tray.is_ready() {
            self.tray.clear_error();
            return true;
        }

        match crate::tray::build() {
            Ok((icon, menu_ids)) => {
                let id = icon.id().clone();
                self.tray.icon = Some(icon);
                self.tray.id = Some(id);
                self.tray.menu_ids = Some(menu_ids);
                self.tray.init_error = None;
                self.runtime_notice = None;
                true
            }
            Err(error) => {
                self.tray.icon = None;
                self.tray.id = None;
                self.tray.menu_ids = None;
                self.tray.init_error = Some(error.clone());
                self.runtime_notice = Some(error);
                false
            }
        }
    }

    fn open_panel_window(
        &mut self,
        visible: bool,
        skip_taskbar: bool,
        position: Option<iced::Point>,
    ) -> Task<Message> {
        self.panel.visible = visible;

        let (_, open) = window::open(crate::panel::settings(visible, skip_taskbar, position));

        open.map(Message::PanelOpened)
    }

    fn handle_panel_opened(&mut self, id: window::Id) -> Task<Message> {
        self.panel.id = Some(id);

        window::get_scale_factor(id)
            .map(move |scale_factor| Message::PanelScaleFactorLoaded(id, scale_factor))
    }

    fn poll_shell_events(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();

        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let Some(task) = self.handle_tray_event(event) {
                tasks.push(task);
            }
        }

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some(task) = self.handle_menu_event(event) {
                tasks.push(task);
            }
        }

        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    fn handle_tray_event(&mut self, event: TrayIconEvent) -> Option<Task<Message>> {
        if self.tray.id.as_ref() != Some(event.id()) {
            return None;
        }

        match event {
            TrayIconEvent::Click {
                rect,
                button,
                button_state,
                ..
            } => {
                self.panel.anchor = Some(rect);

                if button == MouseButton::Left && button_state == MouseButtonState::Up {
                    Some(self.toggle_panel())
                } else {
                    None
                }
            }
            TrayIconEvent::DoubleClick { rect, .. }
            | TrayIconEvent::Enter { rect, .. }
            | TrayIconEvent::Move { rect, .. }
            | TrayIconEvent::Leave { rect, .. } => {
                self.panel.anchor = Some(rect);
                None
            }
            _ => None,
        }
    }

    fn handle_menu_event(&mut self, event: MenuEvent) -> Option<Task<Message>> {
        let menu_ids = self.tray.menu_ids.as_ref()?;

        if event.id == menu_ids.open {
            Some(self.show_panel())
        } else if event.id == menu_ids.refresh {
            Some(self.request_refresh(RefreshReason::Manual))
        } else if event.id == menu_ids.quit {
            Some(iced::exit())
        } else {
            None
        }
    }

    fn handle_select_provider(&mut self, kind: ProviderKind) -> Task<Message> {
        self.panel.selected_provider = match kind {
            ProviderKind::Copilot => ProviderKind::Copilot,
            _ => ProviderKind::Codex,
        };
        self.config.selected_provider = Some(self.panel.selected_provider);
        self.persist_config();
        Task::none()
    }

    fn open_config_folder(&mut self) -> Task<Message> {
        match paths::config_dir() {
            Ok(path) => {
                if let Err(error) = fs::create_dir_all(&path) {
                    self.runtime_notice = Some(format!(
                        "Failed to create config directory {}: {error}",
                        path.display()
                    ));
                    return Task::none();
                }

                match Command::new("explorer").arg(&path).spawn() {
                    Ok(_) => {
                        self.runtime_notice = None;
                    }
                    Err(error) => {
                        self.runtime_notice = Some(format!(
                            "Failed to open config directory {}: {error}",
                            path.display()
                        ));
                    }
                }
            }
            Err(error) => {
                self.runtime_notice = Some(error);
            }
        }

        Task::none()
    }

    fn request_refresh(&mut self, reason: RefreshReason) -> Task<Message> {
        if self.refresh.in_flight {
            self.refresh.queued_reason = Some(reason);
            return Task::none();
        }

        self.refresh.in_flight = true;
        self.refresh.last_reason = Some(reason);
        self.refresh.last_started_at = Some(SystemTime::now());
        self.refresh.last_error = None;

        Task::perform(
            async {
                crate::providers::codex::fetch_snapshot()
                    .await
                    .map(|snapshot| vec![snapshot])
            },
            Message::RefreshFinished,
        )
    }

    fn handle_refresh_finished(
        &mut self,
        result: Result<Vec<ProviderSnapshot>, String>,
    ) -> Task<Message> {
        self.refresh.in_flight = false;
        self.refresh.last_finished_at = Some(SystemTime::now());

        match result {
            Ok(providers) => {
                self.refresh.last_error = None;
                if self.runtime_notice == self.tray.init_error {
                    self.runtime_notice = None;
                }
                self.merge_provider_snapshots(providers);
                self.persist_cache();
            }
            Err(error) => {
                self.refresh.last_error = Some(error.clone());
                self.apply_refresh_failure(&error);
            }
        }

        if let Some(reason) = self.refresh.queued_reason.take() {
            self.request_refresh(reason)
        } else {
            Task::none()
        }
    }

    fn merge_provider_snapshots(&mut self, providers: Vec<ProviderSnapshot>) {
        for snapshot in providers {
            if let Some(existing) = self
                .cache
                .providers
                .iter_mut()
                .find(|existing| existing.kind == snapshot.kind)
            {
                *existing = snapshot;
            } else {
                self.cache.providers.push(snapshot);
            }
        }
    }

    fn persist_cache(&mut self) {
        match paths::cache_file_path() {
            Ok(path) => {
                self.startup.cache_path = Some(path.clone());
                match cache_store::save(&path, &self.cache) {
                    Ok(()) => {
                        self.startup.cache_state = FileLoadState::Loaded;
                    }
                    Err(error) => {
                        self.runtime_notice = Some(error);
                    }
                }
            }
            Err(error) => {
                self.runtime_notice = Some(error);
            }
        }
    }

    fn persist_config(&mut self) {
        match paths::config_file_path() {
            Ok(path) => {
                self.startup.config_path = Some(path.clone());
                match config_store::save(&path, &self.config) {
                    Ok(()) => {
                        self.startup.config_state = FileLoadState::Loaded;
                    }
                    Err(error) => {
                        self.runtime_notice = Some(error);
                    }
                }
            }
            Err(error) => {
                self.runtime_notice = Some(error);
            }
        }
    }

    fn apply_refresh_failure(&mut self, error: &str) {
        let Some(snapshot) = self
            .cache
            .providers
            .iter_mut()
            .find(|snapshot| snapshot.kind == ProviderKind::Codex)
        else {
            self.runtime_notice = Some(error.to_string());
            return;
        };

        snapshot.stale = true;
        snapshot
            .notes
            .retain(|note| !note.starts_with("Refresh error:"));
        snapshot.notes.insert(0, format!("Refresh error: {error}"));

        let too_old = SystemTime::now()
            .duration_since(snapshot.fetched_at)
            .map(|age| age > STALE_GRACE)
            .unwrap_or(false);

        snapshot.unavailable = too_old;
    }

    fn toggle_panel(&mut self) -> Task<Message> {
        if self.panel.visible {
            self.hide_panel()
        } else {
            self.show_panel()
        }
    }

    fn show_panel(&mut self) -> Task<Message> {
        let position = self.panel_anchor_point();

        if let Some(id) = self.panel.id {
            self.panel.visible = true;

            let mut task = Task::none();

            if let Some(position) = position {
                task = window::move_to(id, position);
            }

            task.chain(window::change_mode(id, window::Mode::Windowed))
                .chain(window::gain_focus(id))
                .chain(Task::done(Message::RefreshRequested(
                    RefreshReason::PanelOpened,
                )))
        } else {
            self.open_panel_window(true, self.should_skip_taskbar(), position)
                .chain(Task::done(Message::RefreshRequested(
                    RefreshReason::PanelOpened,
                )))
        }
    }

    fn hide_panel(&mut self) -> Task<Message> {
        if !self.tray.is_ready() {
            return iced::exit();
        }

        let Some(id) = self.panel.id else {
            return Task::none();
        };

        self.panel.visible = false;
        window::change_mode(id, window::Mode::Hidden)
    }

    fn should_skip_taskbar(&self) -> bool {
        self.tray.is_ready() && self.config.start_in_tray
    }

    fn panel_anchor_point(&self) -> Option<iced::Point> {
        self.panel
            .anchor
            .map(|rect| crate::panel::anchor_point(rect, self.panel.scale_factor))
    }

    fn top_bar_view(&self) -> Element<'_, Message> {
        row![
            text(self.refresh_status_text())
                .size(12)
                .color(color_muted())
                .width(Length::Fill),
            icon_button(
                "↻",
                Message::RefreshRequested(RefreshReason::Manual),
                Tone::Neutral,
            ),
            icon_button("×", Message::HidePanel, Tone::Neutral),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    }

    fn selected_provider_view(&self) -> Element<'_, Message> {
        provider_card(self.provider_card_data(self.panel.selected_provider))
    }

    fn bottom_bar_view(&self) -> Element<'_, Message> {
        let tabs = row![
            provider_tab_button(
                ProviderKind::Codex,
                self.panel.selected_provider == ProviderKind::Codex,
                self.provider_tab_tone(ProviderKind::Codex),
            ),
            provider_tab_button(
                ProviderKind::Copilot,
                self.panel.selected_provider == ProviderKind::Copilot,
                self.provider_tab_tone(ProviderKind::Copilot),
            ),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .width(Length::Fill);

        container(
            row![
                tabs,
                icon_button("⚙", Message::OpenConfigFolder, Tone::Neutral)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([8, 10])
        .style(bottom_bar_style)
        .into()
    }

    fn provider_tab_tone(&self, kind: ProviderKind) -> Tone {
        self.snapshot(kind)
            .and_then(|snapshot| snapshot.summary_bar.as_ref())
            .map(|bar| tone_for_percent_left(bar.percent_left))
            .unwrap_or(Tone::Neutral)
    }

    fn provider_card_data(&self, kind: ProviderKind) -> ProviderCardData {
        let Some(snapshot) = self.snapshot(kind) else {
            return match kind {
                ProviderKind::Codex => ProviderCardData {
                    title: kind.label(),
                    tone: Tone::Neutral,
                    badge: if self.refresh.in_flight {
                        "Refreshing".to_string()
                    } else {
                        "Waiting".to_string()
                    },
                    headline: if self.refresh.in_flight {
                        "Checking usage now".to_string()
                    } else {
                        "No local snapshot yet".to_string()
                    },
                    detail: "The card will fill in once the Codex usage source responds.".to_string(),
                    percent_used: None,
                    detail_bars: Vec::new(),
                },
                ProviderKind::Copilot => ProviderCardData {
                    title: kind.label(),
                    tone: Tone::Neutral,
                    badge: "Unavailable".to_string(),
                    headline: "Support not wired yet".to_string(),
                    detail: "Copilot stays visible here, but it will not pretend to know usage until a trustworthy source exists.".to_string(),
                    percent_used: None,
                    detail_bars: Vec::new(),
                },
                _ => ProviderCardData {
                    title: kind.label(),
                    tone: Tone::Neutral,
                    badge: "Unavailable".to_string(),
                    headline: "Not available".to_string(),
                    detail: "No support is currently wired for this provider.".to_string(),
                    percent_used: None,
                    detail_bars: Vec::new(),
                },
            };
        };

        if snapshot.unavailable {
            return ProviderCardData {
                title: kind.label(),
                tone: Tone::Neutral,
                badge: if snapshot.stale {
                    "Unavailable".to_string()
                } else {
                    "No value".to_string()
                },
                headline: "No trustworthy value available".to_string(),
                detail: first_meaningful_note(snapshot).unwrap_or_else(|| {
                    "The provider is expected, but no reliable snapshot is available yet."
                        .to_string()
                }),
                percent_used: None,
                detail_bars: Vec::new(),
            };
        }

        if let Some(bar) = snapshot.summary_bar.as_ref() {
            let tone = tone_for_percent_left(bar.percent_left);

            return ProviderCardData {
                title: kind.label(),
                tone: if snapshot.stale { Tone::Warning } else { tone },
                badge: if snapshot.stale {
                    "Stale".to_string()
                } else {
                    format!("{:.0}% left", bar.percent_left)
                },
                headline: format!(
                    "{:.0}% left · {:.0}% used",
                    bar.percent_left, bar.percent_used
                ),
                detail: if snapshot.stale {
                    first_meaningful_note(snapshot).unwrap_or_else(|| {
                        "Showing the last known value until refresh succeeds.".to_string()
                    })
                } else {
                    format_reset_text(bar.reset_at)
                },
                percent_used: Some(bar.percent_used),
                detail_bars: snapshot
                    .detail_bars
                    .iter()
                    .map(|bar| DetailBarData {
                        label: bar.label.clone(),
                        detail: if let Some(subtitle) = bar.subtitle.as_ref() {
                            format!("{:.0}% left · {subtitle}", bar.percent_left)
                        } else {
                            format!(
                                "{:.0}% left · {}",
                                bar.percent_left,
                                format_reset_text(bar.reset_at)
                            )
                        },
                        percent_used: bar.percent_used,
                        tone: tone_for_percent_left(bar.percent_left),
                    })
                    .collect(),
            };
        }

        ProviderCardData {
            title: kind.label(),
            tone: if snapshot.stale {
                Tone::Warning
            } else {
                Tone::Neutral
            },
            badge: if snapshot.stale {
                "Stale".to_string()
            } else {
                "Partial".to_string()
            },
            headline: "Partial snapshot available".to_string(),
            detail: first_meaningful_note(snapshot).unwrap_or_else(|| {
                "Some provider data is present, but the summary row is not ready yet.".to_string()
            }),
            percent_used: None,
            detail_bars: Vec::new(),
        }
    }

    fn snapshot(&self, kind: ProviderKind) -> Option<&ProviderSnapshot> {
        self.cache
            .providers
            .iter()
            .find(|snapshot| snapshot.kind == kind)
    }

    fn refresh_status_text(&self) -> String {
        if self.refresh.in_flight {
            "Refreshing…".to_string()
        } else if self.refresh.last_error.is_some() {
            "Refresh failed · showing last known value".to_string()
        } else if let Some(when) = self.refresh.last_finished_at {
            format!("Updated {}", relative_time_text(when))
        } else {
            "Waiting for first refresh".to_string()
        }
    }

    fn notice_text(&self) -> Option<String> {
        if let Some(notice) = &self.runtime_notice {
            Some(notice.clone())
        } else if !self.startup.notes.is_empty() {
            Some(self.startup.notes.join("  •  "))
        } else {
            None
        }
    }

    fn notice_tone(&self) -> Tone {
        if self.runtime_notice.is_some() {
            Tone::Warning
        } else {
            Tone::Neutral
        }
    }
}

#[derive(Debug, Clone)]
struct ProviderCardData {
    title: &'static str,
    tone: Tone,
    badge: String,
    headline: String,
    detail: String,
    percent_used: Option<f32>,
    detail_bars: Vec<DetailBarData>,
}

#[derive(Debug, Clone)]
struct DetailBarData {
    label: String,
    detail: String,
    percent_used: f32,
    tone: Tone,
}

#[derive(Debug, Clone, Copy)]
enum Tone {
    Neutral,
    Success,
    Warning,
    Danger,
}

fn handle_escape_key(event: Event, _status: event::Status, window: window::Id) -> Option<Message> {
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => match key {
            keyboard::Key::Named(keyboard::key::Named::Escape) => {
                Some(Message::EscapePressed(window))
            }
            _ => None,
        },
        _ => None,
    }
}

fn provider_card(data: ProviderCardData) -> Element<'static, Message> {
    let badge = status_badge_owned(data.badge, data.tone);

    let mut body = column![
        row![text(data.title).size(17).color(color_text()), badge]
            .spacing(8)
            .align_y(Alignment::Center)
            .wrap(),
        text(data.headline).size(14).color(color_text()),
        text(data.detail).size(12).color(color_muted()),
    ]
    .spacing(7);

    if let Some(percent_used) = data.percent_used {
        body = body.push(
            progress_bar(0.0..=100.0, percent_used)
                .height(5)
                .style(move |_theme| progress_style(data.tone)),
        );
    }

    for detail in data.detail_bars {
        body = body.push(detail_bar(detail));
    }

    container(body)
        .width(Length::Fill)
        .padding(12)
        .style(move |_theme| provider_card_style(data.tone))
        .into()
}

fn detail_bar(data: DetailBarData) -> Element<'static, Message> {
    column![
        text(data.label).size(12).color(color_text()),
        text(data.detail).size(11).color(color_muted()),
        progress_bar(0.0..=100.0, data.percent_used)
            .height(4)
            .style(move |_theme| progress_style(data.tone)),
    ]
    .spacing(4)
    .into()
}

fn provider_tab_button(
    kind: ProviderKind,
    active: bool,
    tone: Tone,
) -> iced::widget::Button<'static, Message> {
    button(text(compact_provider_label(kind)).size(12))
        .padding([6, 10])
        .style(move |_theme, status| provider_tab_style(active, tone, status))
        .on_press(Message::SelectProvider(kind))
}

fn icon_button(
    label: &'static str,
    message: Message,
    tone: Tone,
) -> iced::widget::Button<'static, Message> {
    button(text(label).size(13).color(color_text()))
        .padding([5, 8])
        .style(move |_theme, status| icon_button_style(tone, status))
        .on_press(message)
}

fn status_badge_owned(label: String, tone: Tone) -> Element<'static, Message> {
    let colors = tone_colors(tone);

    container(text(label).size(10).color(colors.text))
        .padding([3, 7])
        .style(move |_theme| iced::widget::container::Style {
            background: Some(colors.background.into()),
            border: Border {
                width: 1.0,
                radius: 999.0.into(),
                color: colors.border,
            },
            ..Default::default()
        })
        .into()
}

fn notice_view(message: String, tone: Tone) -> Element<'static, Message> {
    let colors = tone_colors(tone);

    container(text(message).size(11).color(colors.text))
        .width(Length::Fill)
        .padding(10)
        .style(move |_theme| iced::widget::container::Style {
            background: Some(surface_raised().into()),
            border: Border {
                width: 1.0,
                radius: 10.0.into(),
                color: colors.border,
            },
            ..Default::default()
        })
        .into()
}

fn compact_provider_label(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Codex => "Codex",
        ProviderKind::Copilot => "Copilot",
        ProviderKind::ClaudeCode => "Claude",
        ProviderKind::GeminiCli => "Gemini",
    }
}

fn first_meaningful_note(snapshot: &ProviderSnapshot) -> Option<String> {
    snapshot
        .notes
        .iter()
        .find(|note| !note.starts_with("Plan:"))
        .cloned()
        .or_else(|| snapshot.notes.first().cloned())
}

fn format_reset_text(reset_at: Option<SystemTime>) -> String {
    let Some(reset_at) = reset_at else {
        return "Reset time unavailable".to_string();
    };

    match reset_at.duration_since(SystemTime::now()) {
        Ok(duration) if duration.as_secs() < 60 => "Resets in under 1m".to_string(),
        Ok(duration) if duration.as_secs() < 3_600 => {
            format!("Resets in {}m", duration.as_secs() / 60)
        }
        Ok(duration) if duration.as_secs() < 86_400 => {
            format!("Resets in {}h", duration.as_secs() / 3_600)
        }
        Ok(duration) => format!("Resets in {}d", duration.as_secs() / 86_400),
        Err(_) => "Reset pending".to_string(),
    }
}

fn relative_time_text(when: SystemTime) -> String {
    match SystemTime::now().duration_since(when) {
        Ok(duration) if duration.as_secs() < 30 => "just now".to_string(),
        Ok(duration) if duration.as_secs() < 3_600 => {
            format!("{}m ago", duration.as_secs() / 60)
        }
        Ok(duration) if duration.as_secs() < 86_400 => {
            format!("{}h ago", duration.as_secs() / 3_600)
        }
        Ok(duration) => format!("{}d ago", duration.as_secs() / 86_400),
        Err(_) => "just now".to_string(),
    }
}

fn tone_for_percent_left(percent_left: f32) -> Tone {
    if percent_left <= 5.0 {
        Tone::Danger
    } else if percent_left <= 15.0 {
        Tone::Warning
    } else {
        Tone::Success
    }
}

#[derive(Clone, Copy)]
struct ToneColors {
    background: Color,
    text: Color,
    border: Color,
    bar: Color,
}

fn tone_colors(tone: Tone) -> ToneColors {
    match tone {
        Tone::Neutral => ToneColors {
            background: color_rgb(29, 31, 35),
            text: color_rgb(182, 188, 196),
            border: color_rgb(62, 67, 74),
            bar: color_rgb(128, 135, 144),
        },
        Tone::Success => ToneColors {
            background: color_rgb(29, 43, 36),
            text: color_rgb(156, 214, 182),
            border: color_rgb(68, 104, 86),
            bar: color_rgb(102, 190, 141),
        },
        Tone::Warning => ToneColors {
            background: color_rgb(47, 40, 27),
            text: color_rgb(226, 195, 124),
            border: color_rgb(103, 84, 53),
            bar: color_rgb(219, 172, 79),
        },
        Tone::Danger => ToneColors {
            background: color_rgb(52, 30, 33),
            text: color_rgb(235, 168, 176),
            border: color_rgb(120, 68, 75),
            bar: color_rgb(223, 96, 117),
        },
    }
}

fn panel_shell_style(_theme: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(color_rgb(18, 20, 24).into()),
        border: Border {
            width: 1.0,
            radius: 14.0.into(),
            color: color_rgb(60, 64, 70),
        },
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, 0.18),
            offset: iced::Vector::new(0.0, 4.0),
            blur_radius: 14.0,
        },
        ..Default::default()
    }
}

fn provider_card_style(tone: Tone) -> iced::widget::container::Style {
    let colors = tone_colors(tone);

    iced::widget::container::Style {
        background: Some(surface_raised().into()),
        border: Border {
            width: 1.0,
            radius: 12.0.into(),
            color: colors.border,
        },
        ..Default::default()
    }
}

fn bottom_bar_style(_theme: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(surface_primary().into()),
        border: Border {
            width: 1.0,
            radius: 12.0.into(),
            color: color_rgb(60, 64, 70),
        },
        ..Default::default()
    }
}

fn provider_tab_style(active: bool, tone: Tone, status: button::Status) -> button::Style {
    let colors = tone_colors(if active { tone } else { Tone::Neutral });
    let mut background = if active {
        color_rgb(30, 35, 39)
    } else {
        color_rgb(24, 26, 30)
    };
    let mut border = if active {
        colors.border
    } else {
        color_rgb(52, 56, 62)
    };
    let mut text_color = if active { color_text() } else { color_muted() };

    match status {
        button::Status::Hovered => {
            background = if active {
                color_rgb(34, 38, 43)
            } else {
                color_rgb(30, 33, 38)
            };
            text_color = color_text();
        }
        button::Status::Pressed => {
            background = color_rgb(22, 24, 28);
            border = colors.text;
        }
        button::Status::Disabled => {
            text_color = color_rgb(98, 103, 111);
        }
        button::Status::Active => {}
    }

    button::Style {
        background: Some(background.into()),
        text_color,
        border: Border {
            width: 1.0,
            radius: 999.0.into(),
            color: border,
        },
        shadow: Shadow::default(),
    }
}

fn icon_button_style(tone: Tone, status: button::Status) -> button::Style {
    let colors = tone_colors(tone);
    let mut background = color_rgb(24, 26, 30);
    let mut border = color_rgb(50, 54, 60);

    match status {
        button::Status::Hovered => {
            background = color_rgb(31, 34, 39);
            border = colors.border;
        }
        button::Status::Pressed => {
            background = color_rgb(20, 22, 26);
            border = colors.text;
        }
        button::Status::Disabled => {
            return button::Style {
                background: Some(color_rgb(22, 24, 28).into()),
                text_color: color_rgb(98, 103, 111),
                border: Border {
                    width: 1.0,
                    radius: 8.0.into(),
                    color: color_rgb(45, 48, 53),
                },
                shadow: Shadow::default(),
            };
        }
        button::Status::Active => {}
    }

    button::Style {
        background: Some(background.into()),
        text_color: color_text(),
        border: Border {
            width: 1.0,
            radius: 8.0.into(),
            color: border,
        },
        shadow: Shadow::default(),
    }
}

fn progress_style(tone: Tone) -> progress_bar::Style {
    let colors = tone_colors(tone);

    progress_bar::Style {
        background: color_rgb(31, 33, 38).into(),
        bar: colors.bar.into(),
        border: Border {
            width: 0.0,
            radius: 999.0.into(),
            color: Color::TRANSPARENT,
        },
    }
}

fn panel_scrollable_style(
    status: iced::widget::scrollable::Status,
    scrollbar_is_active: bool,
) -> iced::widget::scrollable::Style {
    let active = scroll_rail(
        if scrollbar_is_active { 0.04 } else { 0.0 },
        if scrollbar_is_active { 0.28 } else { 0.08 },
    );
    let hovered = scroll_rail(0.08, 0.62);
    let dragged = scroll_rail(0.12, 0.82);

    match status {
        iced::widget::scrollable::Status::Active => iced::widget::scrollable::Style {
            container: iced::widget::container::Style::default(),
            vertical_rail: active,
            horizontal_rail: active,
            gap: None,
        },
        iced::widget::scrollable::Status::Hovered {
            is_horizontal_scrollbar_hovered,
            is_vertical_scrollbar_hovered,
        } => iced::widget::scrollable::Style {
            container: iced::widget::container::Style::default(),
            vertical_rail: if is_vertical_scrollbar_hovered {
                hovered
            } else {
                active
            },
            horizontal_rail: if is_horizontal_scrollbar_hovered {
                hovered
            } else {
                active
            },
            gap: None,
        },
        iced::widget::scrollable::Status::Dragged {
            is_horizontal_scrollbar_dragged,
            is_vertical_scrollbar_dragged,
        } => iced::widget::scrollable::Style {
            container: iced::widget::container::Style::default(),
            vertical_rail: if is_vertical_scrollbar_dragged {
                dragged
            } else {
                active
            },
            horizontal_rail: if is_horizontal_scrollbar_dragged {
                dragged
            } else {
                active
            },
            gap: None,
        },
    }
}

fn scroll_rail(rail_alpha: f32, scroller_alpha: f32) -> iced::widget::scrollable::Rail {
    iced::widget::scrollable::Rail {
        background: if rail_alpha <= 0.0 {
            None
        } else {
            Some(Color::from_rgba8(255, 255, 255, rail_alpha).into())
        },
        border: Border {
            width: 0.0,
            radius: 999.0.into(),
            color: Color::TRANSPARENT,
        },
        scroller: iced::widget::scrollable::Scroller {
            color: Color::from_rgba8(196, 201, 209, scroller_alpha),
            border: Border {
                width: 0.0,
                radius: 999.0.into(),
                color: Color::TRANSPARENT,
            },
        },
    }
}

fn color_text() -> Color {
    color_rgb(236, 239, 242)
}

fn color_muted() -> Color {
    color_rgb(159, 166, 175)
}

fn surface_primary() -> Color {
    color_rgb(22, 24, 29)
}

fn surface_raised() -> Color {
    color_rgb(27, 30, 35)
}

fn color_rgb(red: u8, green: u8, blue: u8) -> Color {
    Color::from_rgb8(red, green, blue)
}
