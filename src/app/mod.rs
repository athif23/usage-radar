pub mod message;
mod startup;
pub mod state;

use std::fs;
use std::process::Command;
use std::time::{Duration, SystemTime};

use iced::widget::svg;
use iced::widget::{
    button, column, container, horizontal_space, progress_bar, row, scrollable, text,
};
use iced::{
    alignment, event, keyboard, window, Alignment, Border, Color, Element, Event, Font, Length,
    Shadow, Subscription, Task, Theme,
};
use lucide_icons::Icon as LucideIcon;
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
            Message::SelectPage(selected_provider) => self.handle_select_page(selected_provider),
            Message::OpenAbout => {
                self.panel.show_about = true;
                Task::none()
            }
            Message::OpenConfigFolder => self.open_config_folder(),
            Message::RefreshRequested(reason) => self.request_refresh(reason),
            Message::RefreshFinished(result) => self.handle_refresh_finished(result),
            Message::QuitRequested => iced::exit(),
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

        let mut body = column![self.page_content_view()].spacing(10);

        if let Some(notice) = self.notice_text() {
            body = body.push(notice_view(notice, self.notice_tone()));
        }

        let scrollable_body = scrollable(body)
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

        let layout = column![
            self.top_tabs_view(),
            scrollable_body,
            self.bottom_menu_view()
        ]
        .spacing(10)
        .height(Length::Fill);

        container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(12)
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

    fn handle_select_page(&mut self, selected_provider: Option<ProviderKind>) -> Task<Message> {
        self.panel.show_about = false;
        self.panel.selected_provider = selected_provider;
        self.config.selected_provider = selected_provider;
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

    fn top_tabs_view(&self) -> Element<'_, Message> {
        let home_active = !self.panel.show_about && self.panel.selected_provider.is_none();
        let codex_active =
            !self.panel.show_about && self.panel.selected_provider == Some(ProviderKind::Codex);
        let copilot_active =
            !self.panel.show_about && self.panel.selected_provider == Some(ProviderKind::Copilot);
        let gemini_active =
            !self.panel.show_about && self.panel.selected_provider == Some(ProviderKind::GeminiCli);
        let claude_active = !self.panel.show_about
            && self.panel.selected_provider == Some(ProviderKind::ClaudeCode);

        let tabs = row![
            page_tab_button(
                "Home",
                TabIcon::Home,
                home_active,
                Message::SelectPage(None),
                accent_home(),
            ),
            page_tab_button(
                "Codex",
                TabIcon::Codex,
                codex_active,
                Message::SelectPage(Some(ProviderKind::Codex)),
                provider_accent(ProviderKind::Codex),
            ),
            page_tab_button(
                "Copilot",
                TabIcon::Copilot,
                copilot_active,
                Message::SelectPage(Some(ProviderKind::Copilot)),
                provider_accent(ProviderKind::Copilot),
            ),
            page_tab_button(
                "Gemini",
                TabIcon::Gemini,
                gemini_active,
                Message::SelectPage(Some(ProviderKind::GeminiCli)),
                provider_accent(ProviderKind::GeminiCli),
            ),
            page_tab_button(
                "Claude",
                TabIcon::Claude,
                claude_active,
                Message::SelectPage(Some(ProviderKind::ClaudeCode)),
                provider_accent(ProviderKind::ClaudeCode),
            ),
        ]
        .spacing(6)
        .align_y(Alignment::Start);

        column![tabs, divider_line()].spacing(6).into()
    }

    fn page_content_view(&self) -> Element<'_, Message> {
        if self.panel.show_about {
            self.about_page_view()
        } else {
            match self.panel.selected_provider {
                None => self.home_page_view(),
                Some(kind) => self.provider_page_view(kind),
            }
        }
    }

    fn home_page_view(&self) -> Element<'_, Message> {
        column![
            provider_card(self.provider_card_model(ProviderKind::Codex)),
            provider_card(self.provider_card_model(ProviderKind::Copilot)),
        ]
        .spacing(14)
        .into()
    }

    fn provider_page_view(&self, kind: ProviderKind) -> Element<'_, Message> {
        column![provider_panel(self.provider_card_model(kind), false)]
            .spacing(10)
            .into()
    }

    fn about_page_view(&self) -> Element<'_, Message> {
        let version = env!("CARGO_PKG_VERSION");
        let config_path = self
            .startup
            .config_path
            .clone()
            .or_else(|| paths::config_file_path().ok())
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Unavailable".to_string());

        let cache_path = self
            .startup
            .cache_path
            .clone()
            .or_else(|| paths::cache_file_path().ok())
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Unavailable".to_string());

        let card = column![
            text("Usage Radar").size(21).color(color_text()),
            text("Tray-first usage monitor for local AI tools.")
                .size(14)
                .color(color_text()),
            text(
                "Codex is wired first. Copilot, Claude, and Gemini stay honest until trustworthy local sources are implemented."
            )
            .size(13)
            .color(color_muted()),
            divider_line(),
            text(format!("Version {version}")).size(13).color(color_text()),
            text(format!("Config: {config_path}"))
                .size(12)
                .color(color_muted()),
            text(format!("Cache: {cache_path}"))
                .size(12)
                .color(color_muted()),
        ]
        .spacing(8);

        container(card)
            .width(Length::Fill)
            .padding(14)
            .style(|_theme| provider_card_style(color_border()))
            .into()
    }

    fn bottom_menu_view(&self) -> Element<'_, Message> {
        let left_actions = row![
            toolbar_icon_button(LucideIcon::Settings, Message::OpenConfigFolder),
            toolbar_icon_button(
                LucideIcon::RefreshCw,
                Message::RefreshRequested(RefreshReason::Manual),
            ),
            toolbar_icon_button(LucideIcon::CircleHelp, Message::OpenAbout),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        column![
            divider_line(),
            row![
                left_actions,
                horizontal_space(),
                toolbar_icon_button(LucideIcon::X, Message::QuitRequested),
            ]
            .align_y(Alignment::Center)
            .padding([10, 0]),
        ]
        .spacing(0)
        .into()
    }

    fn provider_card_model(&self, kind: ProviderKind) -> ProviderCardModel {
        let title = provider_ui_label(kind);
        let accent = provider_accent(kind);

        let Some(snapshot) = self.snapshot(kind) else {
            return match kind {
                ProviderKind::Codex => ProviderCardModel {
                    title,
                    accent,
                    subtitle: None,
                    sections: Vec::new(),
                    headline: Some(if self.refresh.in_flight {
                        "Checking usage now".to_string()
                    } else {
                        "No local snapshot yet".to_string()
                    }),
                    detail: Some(
                        "The Codex card will fill in as soon as the refresh loop returns data."
                            .to_string(),
                    ),
                },
                _ => ProviderCardModel {
                    title,
                    accent,
                    subtitle: None,
                    sections: Vec::new(),
                    headline: Some("Support not wired yet".to_string()),
                    detail: Some(
                        "This page stays visible, but Usage Radar will not invent data until a trustworthy source exists."
                            .to_string(),
                    ),
                },
            };
        };

        if snapshot.unavailable {
            return ProviderCardModel {
                title,
                accent,
                subtitle: if snapshot.stale {
                    Some("Last known snapshot".to_string())
                } else {
                    None
                },
                sections: Vec::new(),
                headline: Some("No trustworthy value available".to_string()),
                detail: Some(first_meaningful_note(snapshot).unwrap_or_else(|| {
                    "The provider is expected, but no reliable snapshot is available yet."
                        .to_string()
                })),
            };
        }

        let sections = provider_sections(kind, snapshot);

        if sections.is_empty() {
            return ProviderCardModel {
                title,
                accent,
                subtitle: if snapshot.stale {
                    Some("Last known snapshot".to_string())
                } else {
                    None
                },
                sections,
                headline: Some("Snapshot available".to_string()),
                detail: Some(first_meaningful_note(snapshot).unwrap_or_else(|| {
                    "The provider responded, but no displayable sections were returned yet."
                        .to_string()
                })),
            };
        }

        ProviderCardModel {
            title,
            accent,
            subtitle: if snapshot.stale {
                Some("Last known snapshot".to_string())
            } else {
                None
            },
            sections,
            headline: None,
            detail: None,
        }
    }

    fn snapshot(&self, kind: ProviderKind) -> Option<&ProviderSnapshot> {
        self.cache
            .providers
            .iter()
            .find(|snapshot| snapshot.kind == kind)
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
struct ProviderCardModel {
    title: &'static str,
    accent: Color,
    subtitle: Option<String>,
    sections: Vec<ProviderSection>,
    headline: Option<String>,
    detail: Option<String>,
}

#[derive(Debug, Clone)]
struct ProviderSection {
    title: String,
    progress: f32,
    leading: String,
    trailing: String,
    accent: Color,
}

#[derive(Debug, Clone, Copy)]
enum Tone {
    Neutral,
    Warning,
}

#[derive(Debug, Clone, Copy)]
enum TabIcon {
    Home,
    Codex,
    Copilot,
    Gemini,
    Claude,
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

fn page_tab_button(
    label: &'static str,
    icon: TabIcon,
    active: bool,
    message: Message,
    _accent: Color,
) -> Element<'static, Message> {
    let icon_color = if active { color_text() } else { color_muted() };

    let content = column![
        container(tab_icon(icon, icon_color))
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Center),
        text(label)
            .size(13)
            .color(if active { color_text() } else { color_muted() }),
    ]
    .spacing(4)
    .align_x(alignment::Horizontal::Center)
    .width(Length::Fill);

    container(
        button(content)
            .width(Length::Fill)
            .padding([8, 6])
            .style(move |_theme, status| page_tab_style(active, status))
            .on_press(message),
    )
    .width(Length::FillPortion(1))
    .into()
}

fn tab_icon(icon: TabIcon, color: Color) -> Element<'static, Message> {
    svg::Svg::new(tab_icon_handle(icon))
        .width(Length::Fixed(16.0))
        .height(Length::Fixed(16.0))
        .style(move |_theme, _status| svg::Style { color: Some(color) })
        .into()
}

fn tab_icon_handle(icon: TabIcon) -> svg::Handle {
    match icon {
        TabIcon::Home => svg::Handle::from_memory(include_bytes!("../../assets/gauge.svg")),
        TabIcon::Codex => {
            svg::Handle::from_memory(include_bytes!("../../assets/OpenAI-white-monoblossom.svg"))
        }
        TabIcon::Copilot => {
            svg::Handle::from_memory(include_bytes!("../../assets/githubcopilot.svg"))
        }
        TabIcon::Gemini => {
            svg::Handle::from_memory(include_bytes!("../../assets/googlegemini.svg"))
        }
        TabIcon::Claude => svg::Handle::from_memory(include_bytes!("../../assets/claude.svg")),
    }
}

fn provider_card(model: ProviderCardModel) -> Element<'static, Message> {
    provider_panel(model, true)
}

fn provider_panel(model: ProviderCardModel, framed: bool) -> Element<'static, Message> {
    let mut body = column![text(model.title).size(17).color(color_text())].spacing(8);

    if let Some(subtitle) = model.subtitle {
        body = body.push(text(subtitle).size(12).color(color_muted()));
    }

    if let Some(headline) = model.headline {
        body = body.push(text(headline).size(14).color(color_text()));
    }

    if let Some(detail) = model.detail {
        body = body.push(text(detail).size(12).color(color_muted()));
    }

    for section in model.sections {
        body = body.push(provider_section(section));
    }

    if framed {
        container(body)
            .width(Length::Fill)
            .padding(14)
            .style(move |_theme| provider_card_style(model.accent))
            .into()
    } else {
        container(body).width(Length::Fill).padding([2, 4]).into()
    }
}

fn provider_section(section: ProviderSection) -> Element<'static, Message> {
    column![
        text(section.title).size(14).color(color_text()),
        progress_bar(0.0..=100.0, section.progress)
            .height(8)
            .style(move |_theme| progress_style(section.accent)),
        row![
            text(section.leading).size(12).color(color_text()),
            horizontal_space(),
            text(section.trailing).size(12).color(color_muted()),
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(6)
    .into()
}

fn toolbar_icon_button(
    icon: LucideIcon,
    message: Message,
) -> iced::widget::Button<'static, Message> {
    button(lucide_icon(icon))
        .padding(9)
        .style(toolbar_icon_button_style)
        .on_press(message)
}

fn lucide_icon(icon: LucideIcon) -> Element<'static, Message> {
    text(char::from(icon).to_string())
        .font(Font::with_name("lucide"))
        .size(18)
        .color(color_text())
        .into()
}

fn notice_view(message: String, tone: Tone) -> Element<'static, Message> {
    let colors = tone_colors(tone);

    container(text(message).size(12).color(colors.text))
        .width(Length::Fill)
        .padding(12)
        .style(move |_theme| iced::widget::container::Style {
            background: Some(surface_card().into()),
            border: Border {
                width: 1.0,
                radius: 12.0.into(),
                color: colors.border,
            },
            ..Default::default()
        })
        .into()
}

fn provider_sections(kind: ProviderKind, snapshot: &ProviderSnapshot) -> Vec<ProviderSection> {
    if !snapshot.detail_bars.is_empty() {
        snapshot
            .detail_bars
            .iter()
            .map(|bar| ProviderSection {
                title: section_label(kind, &bar.label),
                progress: bar.percent_left.clamp(0.0, 100.0),
                leading: format!("{:.0}% left", bar.percent_left),
                trailing: format_reset_text(bar.reset_at),
                accent: provider_accent(kind),
            })
            .collect()
    } else if let Some(bar) = snapshot.summary_bar.as_ref() {
        vec![ProviderSection {
            title: section_label(kind, &bar.label),
            progress: bar.percent_left.clamp(0.0, 100.0),
            leading: format!("{:.0}% left", bar.percent_left),
            trailing: format_reset_text(bar.reset_at),
            accent: provider_accent(kind),
        }]
    } else {
        Vec::new()
    }
}

fn provider_ui_label(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Codex => "Codex",
        ProviderKind::Copilot => "Copilot",
        ProviderKind::ClaudeCode => "Claude",
        ProviderKind::GeminiCli => "Gemini",
    }
}

fn section_label(kind: ProviderKind, label: &str) -> String {
    match (kind, label) {
        (ProviderKind::Codex, "5h window") => "5h usage limit".to_string(),
        (ProviderKind::Codex, "Weekly window") => "Weekly usage".to_string(),
        (_, "5h window") => "Current usage".to_string(),
        (_, "Weekly window") => "Weekly usage".to_string(),
        _ => label.to_string(),
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

#[derive(Clone, Copy)]
struct ToneColors {
    text: Color,
    border: Color,
}

fn tone_colors(tone: Tone) -> ToneColors {
    match tone {
        Tone::Neutral => ToneColors {
            text: color_muted(),
            border: color_border(),
        },
        Tone::Warning => ToneColors {
            text: color_warning_text(),
            border: color_warning_border(),
        },
    }
}

fn panel_shell_style(_theme: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(surface_shell().into()),
        border: Border {
            width: 1.0,
            radius: 0.0.into(),
            color: color_border(),
        },
        shadow: Shadow::default(),
        ..Default::default()
    }
}

fn provider_card_style(_accent: Color) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(surface_card().into()),
        border: Border {
            width: 1.0,
            radius: 16.0.into(),
            color: color_border(),
        },
        ..Default::default()
    }
}

fn page_tab_style(active: bool, status: button::Status) -> button::Style {
    let mut background = if active {
        accent_home()
    } else {
        Color::TRANSPARENT
    };
    let mut text_color = if active { color_text() } else { color_muted() };

    match status {
        button::Status::Hovered => {
            if !active {
                background = Color::from_rgba8(255, 255, 255, 0.04);
                text_color = color_text();
            }
        }
        button::Status::Pressed => {
            if !active {
                background = Color::from_rgba8(255, 255, 255, 0.07);
            }
        }
        button::Status::Disabled => {
            text_color = Color::from_rgb8(120, 126, 134);
        }
        button::Status::Active => {}
    }

    button::Style {
        background: Some(background.into()),
        text_color,
        border: Border {
            width: 0.0,
            radius: 14.0.into(),
            color: Color::TRANSPARENT,
        },
        shadow: Shadow::default(),
    }
}

fn toolbar_icon_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let (background, border_color, text_color) = match status {
        button::Status::Hovered => (
            Color::from_rgba8(255, 255, 255, 0.10),
            Color::from_rgba8(255, 255, 255, 0.18),
            color_text(),
        ),
        button::Status::Pressed => (
            Color::from_rgba8(255, 255, 255, 0.14),
            Color::from_rgba8(255, 255, 255, 0.24),
            color_text(),
        ),
        button::Status::Disabled => (
            Color::from_rgba8(255, 255, 255, 0.04),
            Color::from_rgba8(255, 255, 255, 0.08),
            Color::from_rgb8(120, 126, 134),
        ),
        button::Status::Active => (
            Color::from_rgba8(255, 255, 255, 0.06),
            Color::from_rgba8(255, 255, 255, 0.12),
            color_text(),
        ),
    };

    button::Style {
        background: Some(background.into()),
        text_color,
        border: Border {
            width: 1.0,
            radius: 999.0.into(),
            color: border_color,
        },
        shadow: Shadow::default(),
    }
}

fn progress_style(accent: Color) -> progress_bar::Style {
    progress_bar::Style {
        background: color_progress_track().into(),
        bar: accent.into(),
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

fn divider_line() -> Element<'static, Message> {
    container(text(""))
        .width(Length::Fill)
        .height(1)
        .style(|_theme| iced::widget::container::Style {
            background: Some(color_divider().into()),
            ..Default::default()
        })
        .into()
}

fn accent_home() -> Color {
    color_rgb(67, 113, 239)
}

fn provider_accent(kind: ProviderKind) -> Color {
    match kind {
        ProviderKind::Codex => color_rgb(67, 113, 239),
        ProviderKind::Copilot => color_rgb(46, 169, 79),
        ProviderKind::ClaudeCode => color_rgb(176, 131, 71),
        ProviderKind::GeminiCli => color_rgb(132, 108, 239),
    }
}

fn color_text() -> Color {
    color_rgb(242, 244, 247)
}

fn color_muted() -> Color {
    color_rgb(177, 183, 191)
}

fn color_border() -> Color {
    color_rgb(76, 79, 85)
}

fn color_divider() -> Color {
    color_rgb(67, 70, 75)
}

fn color_progress_track() -> Color {
    color_rgb(69, 71, 76)
}

fn color_warning_text() -> Color {
    color_rgb(236, 198, 119)
}

fn color_warning_border() -> Color {
    color_rgb(117, 93, 56)
}

fn surface_shell() -> Color {
    color_rgb(40, 40, 42)
}

fn surface_card() -> Color {
    color_rgb(53, 53, 55)
}

fn color_rgb(red: u8, green: u8, blue: u8) -> Color {
    Color::from_rgb8(red, green, blue)
}
