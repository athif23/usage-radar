pub mod message;
mod startup;
pub mod state;

use std::time::{Duration, SystemTime};

use iced::widget::{button, column, container, progress_bar, row, scrollable, text};
use iced::{
    event, keyboard, window, Alignment, Border, Color, Element, Event, Length, Shadow,
    Subscription, Task, Theme,
};
use tray_icon::menu::MenuEvent;
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

use crate::providers::{ProviderKind, ProviderSnapshot};
use crate::storage::cache as cache_store;
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

        let mut content = column![
            self.header_view(),
            self.hero_view(),
            self.providers_view(),
            self.footer_view(),
        ]
        .spacing(12);

        if let Some(notice) = self.notice_text() {
            content = content.push(notice_view(notice, self.notice_tone()));
        }

        container(scrollable(content).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(16)
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
                self.runtime_notice = None;
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

    fn header_view(&self) -> Element<'_, Message> {
        let badges = row![
            status_badge(self.shell_badge_text(), self.shell_badge_tone()),
            status_badge(self.refresh_badge_text(), self.refresh_badge_tone()),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        row![
            column![
                text("Usage Radar").size(28).color(color_text()),
                text("AI limits without dashboard friction")
                    .size(14)
                    .color(color_muted()),
                badges,
            ]
            .spacing(6)
            .width(Length::Fill),
            row![
                action_button(
                    "Refresh",
                    Message::RefreshRequested(RefreshReason::Manual),
                    Tone::Neutral,
                ),
                action_button("Hide", Message::HidePanel, Tone::Danger),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(12)
        .align_y(Alignment::Start)
        .into()
    }

    fn hero_view(&self) -> Element<'_, Message> {
        let hero = column![
            text(self.summary_headline()).size(30).color(color_text()),
            text(self.summary_subtitle()).size(14).color(color_muted()),
            row![
                stat_chip("Tray", self.shell_chip_value()),
                stat_chip("Refresh", self.refresh_chip_value()),
                stat_chip("Dismiss", self.dismiss_chip_value()),
            ]
            .spacing(8),
        ]
        .spacing(10);

        container(hero)
            .width(Length::Fill)
            .padding(16)
            .style(hero_card_style)
            .into()
    }

    fn providers_view(&self) -> Element<'_, Message> {
        column![
            text("PROVIDERS").size(11).color(color_label()),
            provider_card(self.provider_card_data(ProviderKind::Codex)),
            provider_card(self.provider_card_data(ProviderKind::Copilot)),
        ]
        .spacing(10)
        .into()
    }

    fn footer_view(&self) -> Element<'_, Message> {
        let footer = row![
            footer_metric("Config", self.config_footer_value().to_string()),
            footer_metric("Cache", self.cache_footer_value()),
            footer_metric("Panel", self.panel_footer_value().to_string()),
        ]
        .spacing(8);

        container(footer)
            .width(Length::Fill)
            .padding(10)
            .style(footer_card_style)
            .into()
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
                        "No snapshot".to_string()
                    },
                    headline: if self.refresh.in_flight {
                        "Checking Codex usage now".to_string()
                    } else {
                        "No local usage snapshot yet".to_string()
                    },
                    detail: "This card will show the 5h and weekly windows once the Codex source responds.".to_string(),
                    percent_used: None,
                    detail_bars: Vec::new(),
                },
                ProviderKind::Copilot => ProviderCardData {
                    title: kind.label(),
                    tone: Tone::Neutral,
                    badge: "Unavailable".to_string(),
                    headline: "Support not wired yet".to_string(),
                    detail: "Copilot will stay honest about partial or estimated data when it arrives.".to_string(),
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
                detail: snapshot.notes.first().cloned().unwrap_or_else(|| {
                    "The provider is expected, but no reliable snapshot is available yet."
                        .to_string()
                }),
                percent_used: None,
                detail_bars: Vec::new(),
            };
        }

        if let Some(bar) = snapshot.summary_bar.as_ref() {
            let tone = tone_for_percent_left(bar.percent_left);
            let badge = if snapshot.stale {
                "Stale".to_string()
            } else {
                format!("{:.0}% left", bar.percent_left)
            };

            let detail = if snapshot.stale {
                "Showing the last known value until a fresh refresh succeeds.".to_string()
            } else {
                format_reset_text(bar.reset_at)
            };

            return ProviderCardData {
                title: kind.label(),
                tone,
                badge,
                headline: format!(
                    "{:.0}% left · {:.0}% used",
                    bar.percent_left, bar.percent_used
                ),
                detail,
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
            detail: snapshot.notes.first().cloned().unwrap_or_else(|| {
                "Some provider data is present, but the summary bar is not ready yet.".to_string()
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

    fn best_snapshot(&self) -> Option<&ProviderSnapshot> {
        self.cache
            .providers
            .iter()
            .filter(|snapshot| !snapshot.unavailable)
            .filter(|snapshot| snapshot.summary_bar.is_some())
            .min_by(|left, right| {
                let left_percent = left
                    .summary_bar
                    .as_ref()
                    .map(|bar| bar.percent_left)
                    .unwrap_or(101.0);
                let right_percent = right
                    .summary_bar
                    .as_ref()
                    .map(|bar| bar.percent_left)
                    .unwrap_or(101.0);

                left_percent.total_cmp(&right_percent)
            })
    }

    fn summary_headline(&self) -> String {
        let Some(snapshot) = self.best_snapshot() else {
            return if self.refresh.in_flight {
                "Checking Codex usage".to_string()
            } else {
                "No usage data yet".to_string()
            };
        };

        let Some(bar) = snapshot.summary_bar.as_ref() else {
            return format!("{} available", snapshot.kind.label());
        };

        format!("{} · {:.0}% left", snapshot.kind.label(), bar.percent_left)
    }

    fn summary_subtitle(&self) -> String {
        let Some(snapshot) = self.best_snapshot() else {
            return if self.refresh.in_flight {
                "The app is fetching the first Codex snapshot now. Cached data will appear here immediately on future opens.".to_string()
            } else if self.refresh.last_error.is_some() {
                "The shell is live, but Codex refresh failed. Fix auth or connectivity and try again.".to_string()
            } else {
                "The tray shell is live. Open it fast, dismiss with Esc, and wire the first provider without dashboard sprawl.".to_string()
            };
        };

        let Some(bar) = snapshot.summary_bar.as_ref() else {
            return "A provider snapshot exists, but the summary row is not ready yet.".to_string();
        };

        if snapshot.stale {
            format!(
                "Showing the last known {} snapshot while the app waits for a fresh refresh.",
                snapshot.kind.label()
            )
        } else {
            format!("{} · {}", bar.label, format_reset_text(bar.reset_at))
        }
    }

    fn shell_badge_text(&self) -> &'static str {
        if self.tray.is_ready() {
            "Tray active"
        } else if self.tray.init_error.is_some() {
            "Fallback mode"
        } else {
            "Starting"
        }
    }

    fn shell_badge_tone(&self) -> Tone {
        if self.tray.is_ready() {
            Tone::Success
        } else if self.tray.init_error.is_some() {
            Tone::Warning
        } else {
            Tone::Neutral
        }
    }

    fn refresh_badge_text(&self) -> &'static str {
        if self.refresh.in_flight {
            "Refreshing"
        } else if self.refresh.last_error.is_some() {
            "Refresh error"
        } else if self.refresh.last_finished_at.is_some() {
            "Updated"
        } else {
            "Waiting"
        }
    }

    fn refresh_badge_tone(&self) -> Tone {
        if self.refresh.in_flight {
            Tone::Neutral
        } else if self.refresh.last_error.is_some() {
            Tone::Warning
        } else if self.refresh.last_finished_at.is_some() {
            Tone::Success
        } else {
            Tone::Neutral
        }
    }

    fn shell_chip_value(&self) -> String {
        if self.tray.is_ready() {
            "Ready".to_string()
        } else {
            "Fallback".to_string()
        }
    }

    fn refresh_chip_value(&self) -> String {
        if self.refresh.in_flight {
            return "In flight".to_string();
        }

        if let Some(when) = self.refresh.last_finished_at {
            return relative_time_text(when);
        }

        if self.refresh.last_error.is_some() {
            return "Failed".to_string();
        }

        "Pending".to_string()
    }

    fn dismiss_chip_value(&self) -> String {
        if self.tray.is_ready() {
            "Esc hides".to_string()
        } else {
            "Esc exits".to_string()
        }
    }

    fn config_footer_value(&self) -> &'static str {
        match self.startup.config_state {
            FileLoadState::Loaded => "Loaded",
            FileLoadState::Missing => "Defaults",
            FileLoadState::Defaulted => "Fallback",
            FileLoadState::NotChecked => "Pending",
        }
    }

    fn cache_footer_value(&self) -> String {
        match self.cache.providers.len() {
            0 => "Empty".to_string(),
            1 => "1 snapshot".to_string(),
            count => format!("{count} snapshots"),
        }
    }

    fn panel_footer_value(&self) -> &'static str {
        if self.panel.anchor.is_some() {
            "Anchored"
        } else {
            "Floating"
        }
    }

    fn notice_text(&self) -> Option<String> {
        if let Some(notice) = &self.runtime_notice {
            Some(notice.clone())
        } else if let Some(error) = &self.refresh.last_error {
            Some(error.clone())
        } else if !self.startup.notes.is_empty() {
            Some(self.startup.notes.join("  •  "))
        } else {
            None
        }
    }

    fn notice_tone(&self) -> Tone {
        if self.runtime_notice.is_some() {
            Tone::Warning
        } else if self.refresh.last_error.is_some() {
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
        row![text(data.title).size(18).color(color_text()), badge,]
            .align_y(Alignment::Center)
            .spacing(10),
        text(data.headline).size(15).color(color_text()),
        text(data.detail).size(13).color(color_muted()),
    ]
    .spacing(8);

    if let Some(percent_used) = data.percent_used {
        body = body.push(
            progress_bar(0.0..=100.0, percent_used)
                .height(6)
                .style(move |_theme| progress_style(data.tone)),
        );
    }

    for detail in data.detail_bars {
        body = body.push(detail_bar(detail));
    }

    container(body)
        .width(Length::Fill)
        .padding(14)
        .style(move |_theme| provider_card_style(data.tone))
        .into()
}

fn detail_bar(data: DetailBarData) -> Element<'static, Message> {
    column![
        row![
            text(data.label).size(12).color(color_text()),
            text(data.detail).size(12).color(color_muted()),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        progress_bar(0.0..=100.0, data.percent_used)
            .height(4)
            .style(move |_theme| progress_style(data.tone)),
    ]
    .spacing(4)
    .into()
}

fn action_button<'a>(
    label: &'a str,
    message: Message,
    tone: Tone,
) -> iced::widget::Button<'a, Message> {
    button(text(label).size(13))
        .padding([8, 12])
        .style(move |_theme, status| action_button_style(tone, status))
        .on_press(message)
}

fn status_badge(label: &'static str, tone: Tone) -> Element<'static, Message> {
    status_badge_owned(label.to_string(), tone)
}

fn status_badge_owned(label: String, tone: Tone) -> Element<'static, Message> {
    let colors = tone_colors(tone);

    container(text(label).size(11).color(colors.text))
        .padding([4, 8])
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

fn stat_chip(label: &'static str, value: String) -> Element<'static, Message> {
    container(
        column![
            text(value).size(13).color(color_text()),
            text(label).size(11).color(color_label()),
        ]
        .spacing(2),
    )
    .padding([8, 10])
    .style(chip_style)
    .into()
}

fn footer_metric(label: &'static str, value: String) -> Element<'static, Message> {
    container(
        column![
            text(value).size(13).color(color_text()),
            text(label).size(11).color(color_label()),
        ]
        .spacing(2),
    )
    .width(Length::Fill)
    .padding([8, 10])
    .style(chip_style)
    .into()
}

fn notice_view(message: String, tone: Tone) -> Element<'static, Message> {
    let colors = tone_colors(tone);

    container(text(message).size(12).color(colors.text))
        .width(Length::Fill)
        .padding(12)
        .style(move |_theme| iced::widget::container::Style {
            background: Some(surface_raised().into()),
            border: Border {
                width: 1.0,
                radius: 12.0.into(),
                color: colors.border,
            },
            ..Default::default()
        })
        .into()
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
            background: color_rgb(31, 34, 39),
            text: color_rgb(180, 186, 194),
            border: color_rgb(74, 79, 87),
            bar: color_rgb(132, 139, 148),
        },
        Tone::Success => ToneColors {
            background: color_rgb(31, 47, 41),
            text: color_rgb(155, 213, 181),
            border: color_rgb(71, 108, 90),
            bar: color_rgb(103, 190, 142),
        },
        Tone::Warning => ToneColors {
            background: color_rgb(53, 43, 27),
            text: color_rgb(228, 194, 121),
            border: color_rgb(117, 93, 56),
            bar: color_rgb(226, 176, 79),
        },
        Tone::Danger => ToneColors {
            background: color_rgb(58, 31, 34),
            text: color_rgb(235, 165, 174),
            border: color_rgb(125, 67, 76),
            bar: color_rgb(223, 96, 117),
        },
    }
}

fn panel_shell_style(_theme: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(color_rgb(19, 21, 25).into()),
        border: Border {
            width: 1.0,
            radius: 16.0.into(),
            color: color_rgb(67, 71, 78),
        },
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, 0.22),
            offset: iced::Vector::new(0.0, 6.0),
            blur_radius: 18.0,
        },
        ..Default::default()
    }
}

fn hero_card_style(_theme: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(surface_primary().into()),
        border: Border {
            width: 1.0,
            radius: 14.0.into(),
            color: color_rgb(71, 76, 83),
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
            radius: 14.0.into(),
            color: colors.border,
        },
        ..Default::default()
    }
}

fn footer_card_style(_theme: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(surface_primary().into()),
        border: Border {
            width: 1.0,
            radius: 12.0.into(),
            color: color_rgb(71, 76, 83),
        },
        ..Default::default()
    }
}

fn chip_style(_theme: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(color_rgb(29, 31, 36).into()),
        border: Border {
            width: 1.0,
            radius: 10.0.into(),
            color: color_rgb(64, 68, 75),
        },
        ..Default::default()
    }
}

fn action_button_style(tone: Tone, status: button::Status) -> button::Style {
    let colors = tone_colors(tone);
    let mut background = match tone {
        Tone::Danger => color_rgb(33, 26, 29),
        _ => color_rgb(30, 33, 39),
    };
    let mut border = colors.border;

    match status {
        button::Status::Hovered => {
            background = match tone {
                Tone::Danger => color_rgb(49, 31, 36),
                _ => color_rgb(39, 43, 50),
            };
        }
        button::Status::Pressed => {
            background = match tone {
                Tone::Danger => color_rgb(67, 36, 42),
                _ => color_rgb(25, 28, 33),
            };
            border = colors.text;
        }
        button::Status::Disabled => {
            return button::Style {
                background: Some(color_rgb(24, 26, 30).into()),
                text_color: color_rgb(98, 103, 111),
                border: Border {
                    width: 1.0,
                    radius: 10.0.into(),
                    color: color_rgb(49, 52, 58),
                },
                shadow: Shadow::default(),
            };
        }
        button::Status::Active => {}
    }

    button::Style {
        background: Some(background.into()),
        text_color: colors.text,
        border: Border {
            width: 1.0,
            radius: 10.0.into(),
            color: border,
        },
        shadow: Shadow::default(),
    }
}

fn progress_style(tone: Tone) -> progress_bar::Style {
    let colors = tone_colors(tone);

    progress_bar::Style {
        background: color_rgb(34, 36, 42).into(),
        bar: colors.bar.into(),
        border: Border {
            width: 0.0,
            radius: 999.0.into(),
            color: Color::TRANSPARENT,
        },
    }
}

fn color_text() -> Color {
    color_rgb(235, 238, 242)
}

fn color_muted() -> Color {
    color_rgb(164, 170, 180)
}

fn color_label() -> Color {
    color_rgb(124, 131, 141)
}

fn surface_primary() -> Color {
    color_rgb(24, 27, 32)
}

fn surface_raised() -> Color {
    color_rgb(28, 31, 36)
}

fn color_rgb(red: u8, green: u8, blue: u8) -> Color {
    Color::from_rgb8(red, green, blue)
}
