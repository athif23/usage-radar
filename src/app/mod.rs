pub mod message;
mod startup;
pub mod state;

use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, SystemTime};

use iced::font;
use iced::widget::svg;
use iced::widget::{
    button, column, container, horizontal_space, mouse_area, progress_bar, row, scrollable, text,
    text_input,
};
use iced::{
    alignment, clipboard, event, keyboard, mouse, window, Alignment, Border, Color, Element, Event,
    Font, Length, Padding, Shadow, Subscription, Task, Theme,
};
use lucide_icons::Icon as LucideIcon;
use tray_icon::menu::MenuEvent;
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

use crate::providers::{self, Confidence, CreditBalance, ProviderKind, ProviderSnapshot};
use crate::storage::config::AppAppearance;
use crate::storage::{cache as cache_store, config as config_store};
use crate::util::{paths, startup as startup_util};

use self::message::Message;
use self::startup::load_startup;
pub use self::state::App;
use self::state::{FileLoadState, RefreshReason};

const STALE_GRACE: Duration = Duration::from_secs(30 * 60);
const TRACKABLE_PROVIDERS: [ProviderKind; 3] = [
    ProviderKind::Codex,
    ProviderKind::Copilot,
    ProviderKind::OpenCodeGo,
];
const REFRESH_INTERVAL_OPTIONS: [u64; 4] = [0, 1, 5, 15];
const APPEARANCE_OPTIONS: [AppAppearance; 2] = [AppAppearance::Light, AppAppearance::Dark];
static CURRENT_APPEARANCE: AtomicU8 = AtomicU8::new(0);

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
        set_current_appearance(self.config.appearance);
        match self.config.appearance {
            AppAppearance::Light => Theme::Light,
            AppAppearance::Dark => Theme::Dark,
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            iced::time::every(Duration::from_millis(75)).map(|_| Message::Tick),
            window::close_requests().map(Message::PanelCloseRequested),
            window::close_events().map(Message::PanelClosed),
            event::listen_with(handle_panel_focus_event),
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
            Message::StartPanelDrag => self.start_panel_drag(),
            Message::PanelFocusChanged(id, focused) => self.handle_panel_focus_changed(id, focused),
            Message::SelectPage(selected_provider) => self.handle_select_page(selected_provider),
            Message::BackToMain => {
                self.panel.show_about = false;
                self.panel.show_settings = false;
                Task::none()
            }
            Message::OpenAbout => {
                self.panel.show_about = true;
                self.panel.show_settings = false;
                Task::none()
            }
            Message::OpenSettings => {
                self.panel.show_about = false;
                self.panel.show_settings = true;
                self.panel.show_open_code_go_setup = false;
                Task::none()
            }
            Message::OpenConfigFolder => self.open_config_folder(),
            Message::SetRefreshMinutes(minutes) => self.set_refresh_minutes(minutes),
            Message::SetAppearance(appearance) => self.set_appearance(appearance),
            Message::ToggleLaunchAtStartup => self.toggle_launch_at_startup(),
            Message::ToggleHomeUrgencySort => self.toggle_home_urgency_sort(),
            Message::ToggleProvider(kind) => self.toggle_provider(kind),
            Message::ShowCodexCookieSetup => {
                self.panel.show_codex_cookie_setup = true;
                Task::none()
            }
            Message::HideCodexCookieSetup => {
                self.panel.show_codex_cookie_setup = false;
                Task::none()
            }
            Message::ClearCodexSettings => self.clear_codex_settings(),
            Message::OpenChatGptBillingProbe => self.open_chatgpt_billing_probe(),
            Message::ChatGptBillingProbeFinished(result) => {
                self.handle_chatgpt_billing_probe_finished(result)
            }
            Message::OpenOpenCodeGo => self.open_open_code_go(),
            Message::ShowOpenCodeGoSetup => {
                self.panel.show_open_code_go_setup = true;
                Task::none()
            }
            Message::HideOpenCodeGoSetup => {
                self.panel.show_open_code_go_setup = false;
                Task::none()
            }
            Message::OpenCodeGoCookieHeaderChanged(value) => {
                self.config.opencode_go_cookie_header = Some(value);
                Task::none()
            }
            Message::OpenCodeGoWorkspaceIdChanged(value) => {
                self.config.opencode_go_workspace_id = Some(value);
                Task::none()
            }
            Message::SaveOpenCodeGoSettings => self.save_open_code_go_settings(),
            Message::ClearOpenCodeGoSettings => self.clear_open_code_go_settings(),
            Message::OpenCopilotVerification => self.open_copilot_verification(),
            Message::CopyCopilotCode => self.copy_copilot_code(),
            Message::CopilotConnectRequested => self.start_copilot_sign_in(),
            Message::CopilotSignOutRequested => self.sign_out_copilot(),
            Message::CopilotDeviceCodeReceived(flow_id, result) => {
                self.handle_copilot_device_code_received(flow_id, result)
            }
            Message::CopilotSignInFinished(flow_id, result) => {
                self.handle_copilot_sign_in_finished(flow_id, result)
            }
            Message::RefreshRequested(reason) => self.request_refresh(reason),
            Message::RefreshFinished(outcomes) => self.handle_refresh_finished(outcomes),
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
                    self.panel.has_focus = false;
                    self.panel.last_unfocused_at = None;
                }

                Task::none()
            }
        }
    }

    pub fn view(&self, window_id: window::Id) -> Element<'_, Message> {
        if self.panel.id != Some(window_id) {
            return container(text("")).into();
        }

        set_current_appearance(self.config.appearance);

        let mut body = column![self.page_content_view()].spacing(8);

        if let Some(notice) = self.notice_text() {
            body = body.push(notice_view(notice, self.notice_tone()));
        }

        let scrollable_body = scrollable(
            container(body)
                .width(Length::Fill)
                .padding(Padding::ZERO.bottom(10.0)),
        )
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

        let layout = if self.panel.show_settings || self.panel.show_about {
            column![
                self.auxiliary_page_header_view(),
                scrollable_body,
                self.bottom_menu_view()
            ]
            .spacing(9)
            .height(Length::Fill)
        } else {
            column![
                self.top_tabs_view(),
                scrollable_body,
                self.bottom_menu_view()
            ]
            .spacing(9)
            .height(Length::Fill)
        };

        container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding::ZERO.top(4.0).right(4.0).bottom(5.0).left(4.0))
            .style(panel_shell_style)
            .into()
    }

    fn handle_app_started(&mut self) -> Task<Message> {
        self.sync_copilot_saved_token_state();

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
        self.panel.has_focus = self.panel.visible;
        self.panel.last_unfocused_at = None;

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

    fn handle_panel_focus_changed(&mut self, id: window::Id, focused: bool) -> Task<Message> {
        if self.panel.id == Some(id) {
            self.panel.note_focus_changed(focused);
        }

        Task::none()
    }

    fn handle_select_page(&mut self, selected_provider: Option<ProviderKind>) -> Task<Message> {
        self.panel.show_about = false;
        self.panel.show_settings = false;

        if selected_provider != Some(ProviderKind::OpenCodeGo) {
            self.panel.show_open_code_go_setup = false;
        }
        if selected_provider != Some(ProviderKind::Codex) {
            self.panel.show_codex_cookie_setup = false;
        }

        self.panel.selected_provider = selected_provider;
        self.config.selected_provider = selected_provider;
        self.persist_config();
        Task::none()
    }

    fn set_refresh_minutes(&mut self, minutes: u64) -> Task<Message> {
        self.config.refresh_minutes = minutes;
        self.persist_config();
        Task::none()
    }

    fn set_appearance(&mut self, appearance: AppAppearance) -> Task<Message> {
        self.config.appearance = appearance;
        set_current_appearance(appearance);
        self.persist_config();
        Task::none()
    }

    fn toggle_launch_at_startup(&mut self) -> Task<Message> {
        let next = !self.config.launch_at_startup;

        match startup_util::set_launch_at_startup(next) {
            Ok(()) => {
                self.config.launch_at_startup = next;
                self.config.start_in_tray = true;
                self.persist_config();
            }
            Err(error) => {
                self.runtime_notice = Some(error);
            }
        }

        Task::none()
    }

    fn toggle_home_urgency_sort(&mut self) -> Task<Message> {
        self.config.sort_home_by_urgency = !self.config.sort_home_by_urgency;
        self.persist_config();
        Task::none()
    }

    fn toggle_provider(&mut self, kind: ProviderKind) -> Task<Message> {
        if let Some(index) = self
            .config
            .disabled_providers
            .iter()
            .position(|disabled| *disabled == kind)
        {
            self.config.disabled_providers.remove(index);
            self.persist_config();
            self.request_refresh(RefreshReason::Manual)
        } else {
            self.config.disabled_providers.push(kind);

            if self.panel.selected_provider == Some(kind) {
                self.panel.selected_provider = None;
                self.config.selected_provider = None;
            }

            self.persist_config();
            Task::none()
        }
    }

    fn start_panel_drag(&mut self) -> Task<Message> {
        let Some(id) = self.panel.id else {
            return Task::none();
        };

        window::drag(id)
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

                self.open_external_target(&path.display().to_string(), "config directory")
            }
            Err(error) => {
                self.runtime_notice = Some(error);
                Task::none()
            }
        }
    }

    fn open_open_code_go(&mut self) -> Task<Message> {
        self.open_external_target("https://opencode.ai", "OpenCode Go")
    }

    fn open_chatgpt_billing_probe(&mut self) -> Task<Message> {
        match chatgpt_billing_probe_path() {
            Ok(path) => match Command::new(&path).spawn() {
                Ok(mut child) => {
                    self.panel.show_codex_cookie_setup = false;
                    self.runtime_notice =
                        Some("Update ChatGPT billing balance in the new window.".to_string());
                    Task::perform(
                        async move {
                            let status = child.wait().map_err(|error| {
                                format!("ChatGPT billing balance helper failed: {error}")
                            })?;

                            if status.success() {
                                Ok(())
                            } else {
                                Err(format!(
                                    "ChatGPT billing balance helper exited with {status}"
                                ))
                            }
                        },
                        Message::ChatGptBillingProbeFinished,
                    )
                }
                Err(error) => {
                    self.runtime_notice = Some(format!(
                        "Failed to open ChatGPT billing balance helper {}: {error}",
                        path.display()
                    ));
                    Task::none()
                }
            },
            Err(error) => {
                self.runtime_notice = Some(error);
                Task::none()
            }
        }
    }

    fn handle_chatgpt_billing_probe_finished(
        &mut self,
        result: Result<(), String>,
    ) -> Task<Message> {
        match result {
            Ok(()) => {
                self.runtime_notice =
                    Some("ChatGPT billing balance updated; refreshing Codex.".to_string());
                self.request_refresh(RefreshReason::Manual)
            }
            Err(error) => {
                self.runtime_notice = Some(error);
                Task::none()
            }
        }
    }

    fn clear_codex_settings(&mut self) -> Task<Message> {
        self.config.codex_chatgpt_cookie_header = None;
        self.panel.show_codex_cookie_setup = false;
        if let Ok(path) = paths::chatgpt_billing_file_path() {
            let _ = fs::remove_file(path);
        }
        if let Ok(path) = paths::chatgpt_billing_webview_dir() {
            let _ = fs::remove_dir_all(path);
        }
        self.persist_config();
        self.request_refresh(RefreshReason::Manual)
    }

    fn save_open_code_go_settings(&mut self) -> Task<Message> {
        self.config.opencode_go_cookie_header =
            normalize_optional_config_value(self.config.opencode_go_cookie_header.take());
        self.config.opencode_go_workspace_id =
            normalize_optional_config_value(self.config.opencode_go_workspace_id.take());
        self.panel.show_open_code_go_setup = false;
        self.persist_config();
        self.request_refresh(RefreshReason::Manual)
    }

    fn clear_open_code_go_settings(&mut self) -> Task<Message> {
        self.config.opencode_go_cookie_header = None;
        self.config.opencode_go_workspace_id = None;
        self.panel.show_open_code_go_setup = false;
        self.persist_config();
        self.request_refresh(RefreshReason::Manual)
    }

    fn open_copilot_verification(&mut self) -> Task<Message> {
        let Some(target) = self
            .copilot_auth
            .device_code
            .as_ref()
            .map(|prompt| prompt.verification_url.clone())
        else {
            return Task::none();
        };

        self.open_external_target(&target, "GitHub sign-in page")
    }

    fn start_copilot_sign_in(&mut self) -> Task<Message> {
        if self.copilot_auth.is_busy() || self.copilot_auth.has_saved_token {
            return Task::none();
        }

        self.copilot_auth.flow_id += 1;
        let flow_id = self.copilot_auth.flow_id;
        self.copilot_auth.requesting = true;
        self.copilot_auth.awaiting_snapshot = false;
        self.copilot_auth.device_code = None;
        self.copilot_auth.last_error = None;

        Task::perform(
            async move {
                (
                    flow_id,
                    crate::providers::copilot::request_device_code().await,
                )
            },
            |(flow_id, result)| Message::CopilotDeviceCodeReceived(flow_id, result),
        )
    }

    fn handle_copilot_device_code_received(
        &mut self,
        flow_id: u64,
        result: Result<crate::providers::copilot::DeviceCodePrompt, String>,
    ) -> Task<Message> {
        if flow_id != self.copilot_auth.flow_id {
            return Task::none();
        }

        self.copilot_auth.requesting = false;

        match result {
            Ok(prompt) => {
                self.copilot_auth.last_error = None;
                self.copilot_auth.device_code = Some(prompt.clone());
                let _ = self.open_external_target(&prompt.verification_url, "GitHub sign-in page");

                Task::perform(
                    async move {
                        let result = async {
                            let token = crate::providers::copilot::poll_for_token(&prompt).await?;
                            crate::providers::copilot::store_token(&token)
                        }
                        .await;

                        (flow_id, result)
                    },
                    |(flow_id, result)| Message::CopilotSignInFinished(flow_id, result),
                )
            }
            Err(error) => {
                self.copilot_auth.device_code = None;
                self.copilot_auth.last_error = Some(error);
                Task::none()
            }
        }
    }

    fn handle_copilot_sign_in_finished(
        &mut self,
        flow_id: u64,
        result: Result<(), String>,
    ) -> Task<Message> {
        if flow_id != self.copilot_auth.flow_id {
            return Task::none();
        }

        self.copilot_auth.requesting = false;
        self.copilot_auth.device_code = None;

        match result {
            Ok(()) => {
                self.copilot_auth.awaiting_snapshot = true;
                self.copilot_auth.has_saved_token = true;
                self.copilot_auth.last_error = None;
                self.request_refresh(RefreshReason::Manual)
            }
            Err(error) => {
                self.copilot_auth.awaiting_snapshot = false;
                self.copilot_auth.has_saved_token = false;
                self.copilot_auth.last_error = Some(error);
                Task::none()
            }
        }
    }

    fn sign_out_copilot(&mut self) -> Task<Message> {
        self.copilot_auth.flow_id += 1;

        match crate::providers::copilot::clear_token() {
            Ok(()) => {
                self.copilot_auth.requesting = false;
                self.copilot_auth.awaiting_snapshot = false;
                self.copilot_auth.has_saved_token = false;
                self.copilot_auth.device_code = None;
                self.copilot_auth.last_error = None;
                self.cache
                    .providers
                    .retain(|snapshot| snapshot.kind != ProviderKind::Copilot);
                self.persist_cache();
            }
            Err(error) => {
                self.copilot_auth.last_error = Some(error);
            }
        }

        Task::none()
    }

    fn sync_copilot_saved_token_state(&mut self) {
        match crate::providers::copilot::has_saved_token() {
            Ok(has_saved_token) => {
                self.copilot_auth.has_saved_token = has_saved_token;
            }
            Err(error) => {
                self.copilot_auth.has_saved_token = false;
                self.copilot_auth.last_error = Some(error);
            }
        }
    }

    fn copy_copilot_code(&mut self) -> Task<Message> {
        let Some(code) = self
            .copilot_auth
            .device_code
            .as_ref()
            .map(|prompt| prompt.user_code.clone())
        else {
            return Task::none();
        };

        clipboard::write(code)
    }

    fn open_external_target(&mut self, target: &str, label: &str) -> Task<Message> {
        match spawn_open_target(target) {
            Ok(_) => {
                if self.runtime_notice == self.tray.init_error {
                    self.runtime_notice = None;
                }
            }
            Err(error) => {
                self.runtime_notice = Some(format!("Failed to open {label} {target}: {error}"));
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

        let providers = self.enabled_refresh_providers();
        Task::perform(
            providers::refresh_selected(providers),
            Message::RefreshFinished,
        )
    }

    fn handle_refresh_finished(
        &mut self,
        outcomes: Vec<crate::providers::RefreshOutcome>,
    ) -> Task<Message> {
        self.refresh.in_flight = false;
        self.refresh.last_finished_at = Some(SystemTime::now());

        let mut errors = Vec::new();
        let mut had_success = false;

        for outcome in outcomes {
            if outcome.kind == ProviderKind::Copilot {
                self.copilot_auth.awaiting_snapshot = false;

                if !self.copilot_auth.has_saved_token {
                    continue;
                }
            }

            match outcome.result {
                Ok(snapshot) => {
                    had_success = true;
                    self.merge_provider_snapshots(vec![snapshot]);
                }
                Err(error) => {
                    errors.push(format!("{}: {error}", provider_ui_label(outcome.kind)));
                    self.apply_refresh_failure(outcome.kind, &error);
                }
            }
        }

        if had_success {
            self.persist_cache();
            if self.runtime_notice == self.tray.init_error
                || self.codex_billing_credits_available()
                    && self
                        .runtime_notice
                        .as_deref()
                        .map(|notice| {
                            notice.contains("ChatGPT billing sign-in")
                                || notice.contains("ChatGPT billing balance")
                        })
                        .unwrap_or(false)
            {
                self.runtime_notice = None;
            }
        }

        self.refresh.last_error = if errors.is_empty() {
            None
        } else {
            Some(errors.join("  •  "))
        };

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

    fn apply_refresh_failure(&mut self, kind: ProviderKind, error: &str) {
        let Some(snapshot) = self
            .cache
            .providers
            .iter_mut()
            .find(|snapshot| snapshot.kind == kind)
        else {
            self.cache
                .providers
                .push(provider_failure_snapshot(kind, error));
            return;
        };

        let too_old = SystemTime::now()
            .duration_since(snapshot.fetched_at)
            .map(|age| age > STALE_GRACE)
            .unwrap_or(false);

        snapshot.stale = true;
        snapshot.unavailable = too_old;
        snapshot.notes.retain(|note| {
            !note.starts_with(DISPLAY_NOTE_PREFIX) && !note.starts_with(TECHNICAL_DETAIL_PREFIX)
        });
        snapshot.notes.insert(0, technical_detail_note(error));
        snapshot
            .notes
            .insert(0, display_note(refresh_failure_message(kind, too_old)));
    }

    fn toggle_panel(&mut self) -> Task<Message> {
        if self.panel.visible {
            if self.panel.was_recently_active() {
                self.hide_panel()
            } else {
                self.show_panel()
            }
        } else {
            self.show_panel()
        }
    }

    fn show_panel(&mut self) -> Task<Message> {
        let position = self.panel_anchor_point();

        if let Some(id) = self.panel.id {
            self.panel.visible = true;
            self.panel.has_focus = true;
            self.panel.last_unfocused_at = None;

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
        self.panel.has_focus = false;
        self.panel.last_unfocused_at = None;
        window::change_mode(id, window::Mode::Hidden)
    }

    fn should_skip_taskbar(&self) -> bool {
        self.tray.is_ready() && self.config.start_in_tray
    }

    fn panel_anchor_point(&self) -> Option<iced::Point> {
        crate::panel::open_point(self.panel.anchor, self.panel.scale_factor)
    }

    fn top_tabs_view(&self) -> Element<'_, Message> {
        let provider_page_active = !self.panel.show_about && !self.panel.show_settings;
        let home_active = provider_page_active && self.panel.selected_provider.is_none();
        let codex_active =
            provider_page_active && self.panel.selected_provider == Some(ProviderKind::Codex);
        let copilot_active =
            provider_page_active && self.panel.selected_provider == Some(ProviderKind::Copilot);
        let opencode_active =
            provider_page_active && self.panel.selected_provider == Some(ProviderKind::OpenCodeGo);
        let mut tabs = row![page_tab_button(
            "Home",
            TabIcon::Home,
            home_active,
            Message::SelectPage(None),
            accent_home(),
        ),]
        .spacing(4)
        .align_y(Alignment::Start);

        if self.provider_enabled(ProviderKind::Codex) {
            tabs = tabs.push(page_tab_button(
                "Codex",
                TabIcon::Codex,
                codex_active,
                Message::SelectPage(Some(ProviderKind::Codex)),
                provider_accent(ProviderKind::Codex),
            ));
        }

        if self.provider_enabled(ProviderKind::Copilot) {
            tabs = tabs.push(page_tab_button(
                "Copilot",
                TabIcon::Copilot,
                copilot_active,
                Message::SelectPage(Some(ProviderKind::Copilot)),
                provider_accent(ProviderKind::Copilot),
            ));
        }

        if self.provider_enabled(ProviderKind::OpenCodeGo) {
            tabs = tabs.push(page_tab_button(
                "Go",
                TabIcon::OpenCode,
                opencode_active,
                Message::SelectPage(Some(ProviderKind::OpenCodeGo)),
                provider_accent(ProviderKind::OpenCodeGo),
            ));
        }

        column![tabs, divider_line()].spacing(4).into()
    }

    fn page_content_view(&self) -> Element<'_, Message> {
        if self.panel.show_settings {
            self.settings_page_content_view()
        } else if self.panel.show_about {
            self.about_page_content_view()
        } else {
            match self.panel.selected_provider {
                None => self.home_page_view(),
                Some(kind) => self.provider_page_view(kind),
            }
        }
    }

    fn auxiliary_page_header_view(&self) -> Element<'_, Message> {
        let header = if self.panel.show_settings {
            back_page_header("Settings", Some("Preferences"), None)
        } else {
            back_page_header(
                "About",
                Some("Usage Radar"),
                Some(env!("CARGO_PKG_VERSION")),
            )
        };

        container(header)
            .width(Length::Fill)
            .padding(Padding::ZERO.top(2.0).left(10.0).right(10.0))
            .into()
    }

    fn home_page_view(&self) -> Element<'_, Message> {
        let mut body = column!().spacing(0);
        let mut providers = self.enabled_refresh_providers();

        if self.config.sort_home_by_urgency {
            providers::urgency::sort_by_usage_urgency(&mut providers, &self.cache.providers);
        }

        for (index, kind) in providers.iter().copied().enumerate() {
            if index > 0 {
                body = body.push(divider_line());
            }

            body = body.push(provider_list_row(self.provider_card_model(kind)));
        }

        if providers.is_empty() {
            body = body.push(action_status_card(
                "No providers enabled",
                "Open Settings and enable at least one provider to start checking usage.",
            ));
        }

        container(body)
            .width(Length::Fill)
            .padding(Padding::ZERO.top(4.0).left(10.0).right(10.0))
            .into()
    }

    fn provider_page_view(&self, kind: ProviderKind) -> Element<'_, Message> {
        if kind == ProviderKind::Codex {
            return self.codex_page_view();
        }

        if kind == ProviderKind::Copilot {
            return self.copilot_page_view();
        }

        if kind == ProviderKind::OpenCodeGo {
            return self.open_code_go_page_view();
        }

        container(
            column![
                provider_page_header(provider_ui_label(kind), self.provider_plan_label(kind)),
                provider_panel(self.provider_card_model(kind), false, false)
            ]
            .spacing(8),
        )
        .width(Length::Fill)
        .padding(Padding::ZERO.top(6.0).left(10.0).right(10.0))
        .into()
    }

    fn codex_page_view(&self) -> Element<'_, Message> {
        let snapshot = self.snapshot(ProviderKind::Codex);
        let needs_web_connect = snapshot
            .and_then(|snapshot| snapshot.web_credits.as_ref())
            .map(|credits| credits.remaining.is_none() && !credits.unlimited)
            .unwrap_or(true);
        let setup_visible = self.panel.show_codex_cookie_setup || needs_web_connect;
        let can_collapse_setup = !needs_web_connect;

        let mut body = column![
            provider_page_header(
                provider_ui_label(ProviderKind::Codex),
                self.provider_plan_label(ProviderKind::Codex),
            ),
            provider_panel(self.provider_card_model(ProviderKind::Codex), false, false)
        ]
        .spacing(8);

        if setup_visible {
            body = body.push(chatgpt_billing_setup_card(can_collapse_setup));
        } else {
            body = body.push(text_inline_button(
                "Update OpenAI web balance",
                Message::ShowCodexCookieSetup,
            ));
        }

        container(body)
            .width(Length::Fill)
            .padding(Padding::ZERO.top(6.0).left(10.0).right(10.0))
            .into()
    }

    fn copilot_page_view(&self) -> Element<'_, Message> {
        let mut body = column![copilot_page_header(self.copilot_auth.has_saved_token)].spacing(10);

        if self.copilot_auth.awaiting_snapshot {
            body = body.push(action_status_card(
                "Loading Copilot usage",
                "GitHub sign-in succeeded. Fetching your Copilot usage...",
            ));
        } else if !self.copilot_auth.is_busy() {
            body = body.push(provider_panel(
                self.provider_card_model(ProviderKind::Copilot),
                false,
                false,
            ));
        }

        if self.copilot_auth.requesting {
            body = body.push(action_status_card(
                "Starting GitHub sign-in",
                "Requesting a device code from GitHub...",
            ));
        }

        if let Some(prompt) = self.copilot_auth.device_code.as_ref() {
            body = body.push(copilot_waiting_card(prompt));
        }

        if let Some(error) = self.copilot_auth.last_error.as_ref() {
            body = body.push(action_status_card("GitHub sign-in failed", error));
        }

        if self.should_show_copilot_connect_button() {
            body = body.push(primary_action_button(
                "Sign in with GitHub",
                provider_accent(ProviderKind::Copilot),
                Message::CopilotConnectRequested,
            ));
        }

        container(body)
            .width(Length::Fill)
            .padding(Padding::ZERO.top(3.0).left(10.0).right(10.0))
            .into()
    }

    fn open_code_go_page_view(&self) -> Element<'_, Message> {
        let snapshot = self.snapshot(ProviderKind::OpenCodeGo);
        let manual_override_active = self.open_code_go_manual_override_active();
        let setup_visible = self.open_code_go_setup_visible(snapshot);
        let can_collapse_setup = snapshot
            .map(|snapshot| !snapshot.unavailable)
            .unwrap_or(false);
        let mut body = column![open_code_go_page_header()].spacing(10);

        if let Some(snapshot) = snapshot {
            if !snapshot.unavailable {
                body = body.push(provider_panel(
                    self.provider_card_model(ProviderKind::OpenCodeGo),
                    false,
                    false,
                ));
            } else {
                let detail = first_meaningful_note(snapshot).unwrap_or_else(|| {
                    "Usage Radar will try to import your OpenCode Go session from Chrome, Brave, or Edge on Windows. If that fails, use the manual Cookie fallback below."
                        .to_string()
                });
                body = body.push(action_status_card("OpenCode Go not connected", &detail));
            }
        } else {
            body = body.push(provider_panel(
                self.provider_card_model(ProviderKind::OpenCodeGo),
                false,
                false,
            ));
        }

        if setup_visible {
            body = body.push(open_code_go_setup_card(
                self.config
                    .opencode_go_cookie_header
                    .as_deref()
                    .unwrap_or(""),
                self.config
                    .opencode_go_workspace_id
                    .as_deref()
                    .unwrap_or(""),
                can_collapse_setup,
            ));
        } else if snapshot.is_some() {
            body = body.push(open_code_go_connection_card(
                if manual_override_active {
                    "Manual override active"
                } else {
                    "Connected automatically"
                },
                self.open_code_go_connection_detail(snapshot, manual_override_active),
                manual_override_active,
            ));
        }

        container(body)
            .width(Length::Fill)
            .padding(Padding::ZERO.top(6.0).left(10.0).right(10.0))
            .into()
    }

    fn settings_page_content_view(&self) -> Element<'_, Message> {
        let mut refresh_options = row!().spacing(6).align_y(Alignment::Center);
        for minutes in REFRESH_INTERVAL_OPTIONS {
            refresh_options = refresh_options.push(setting_choice_button(
                refresh_interval_label(minutes),
                self.config.refresh_minutes == minutes,
                Message::SetRefreshMinutes(minutes),
            ));
        }

        let mut appearance_options = row!().spacing(6).align_y(Alignment::Center);
        for appearance in APPEARANCE_OPTIONS {
            appearance_options = appearance_options.push(setting_choice_button(
                appearance_label(appearance),
                self.config.appearance == appearance,
                Message::SetAppearance(appearance),
            ));
        }

        let mut provider_toggles = column!().spacing(6);
        for kind in TRACKABLE_PROVIDERS {
            provider_toggles = provider_toggles.push(setting_toggle_row(
                provider_ui_label(kind),
                provider_settings_detail(kind),
                self.provider_enabled(kind),
                Message::ToggleProvider(kind),
            ));
        }

        let body = column![
            settings_card(column![
                text("Refresh").size(13).color(color_text()),
                refresh_options,
            ]),
            settings_card(column![
                text("Appearance").size(13).color(color_text()),
                appearance_options,
            ]),
            settings_card(column![
                text("Providers").size(13).color(color_text()),
                provider_toggles,
            ]),
            settings_card(column![
                text("Panel").size(13).color(color_text()),
                setting_toggle_row(
                    "Launch at startup",
                    "Open Usage Radar when you sign in",
                    self.config.launch_at_startup,
                    Message::ToggleLaunchAtStartup,
                ),
                divider_line(),
                setting_toggle_row(
                    "Sort by urgency",
                    "Home order stays fixed unless enabled",
                    self.config.sort_home_by_urgency,
                    Message::ToggleHomeUrgencySort,
                ),
            ]),
            settings_card(column![
                text("Files").size(13).color(color_text()),
                settings_menu_row(
                    LucideIcon::FolderOpen,
                    "Open config folder",
                    Message::OpenConfigFolder,
                ),
            ]),
        ]
        .spacing(9);

        container(body)
            .width(Length::Fill)
            .padding(Padding::ZERO.top(6.0).left(10.0).right(10.0))
            .into()
    }

    fn about_page_content_view(&self) -> Element<'_, Message> {
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

        let body = column![
            settings_card(column![
                text("Tray-first usage monitor")
                    .size(13)
                    .color(color_text()),
                text("Local usage visibility for Codex, Copilot, and OpenCode Go.")
                    .size(11)
                    .color(color_muted()),
            ]),
            settings_card(column![
                text("Files").size(13).color(color_text()),
                text(format!("Config: {config_path}"))
                    .size(11)
                    .color(color_muted()),
                text(format!("Cache: {cache_path}"))
                    .size(11)
                    .color(color_muted()),
            ]),
        ]
        .spacing(9);

        container(body)
            .width(Length::Fill)
            .padding(Padding::ZERO.top(6.0).left(10.0).right(10.0))
            .into()
    }

    fn bottom_menu_view(&self) -> Element<'_, Message> {
        let left_actions = row![
            toolbar_icon_button(LucideIcon::Settings, Message::OpenSettings),
            toolbar_icon_button(
                LucideIcon::RefreshCw,
                Message::RefreshRequested(RefreshReason::Manual),
            ),
            toolbar_icon_button(LucideIcon::CircleHelp, Message::OpenAbout),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        column![
            divider_line(),
            mouse_area(
                container(
                    row![
                        left_actions,
                        horizontal_space(),
                        toolbar_icon_button(LucideIcon::X, Message::QuitRequested),
                    ]
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .padding(Padding::ZERO.top(3.0).bottom(0.0)),
            )
            .interaction(mouse::Interaction::Grab)
            .on_press(Message::StartPanelDrag),
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
                    metrics: Vec::new(),
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
                ProviderKind::Copilot => ProviderCardModel {
                    title,
                    accent,
                    subtitle: None,
                    sections: Vec::new(),
                    metrics: Vec::new(),
                    headline: Some(if self.copilot_auth.is_busy() || self.copilot_auth.awaiting_snapshot {
                        "Checking Copilot status".to_string()
                    } else if self.refresh.in_flight || self.copilot_auth.has_saved_token {
                        "Checking Copilot status".to_string()
                    } else {
                        "GitHub sign-in required".to_string()
                    }),
                    detail: Some(if self.copilot_auth.is_busy() || self.copilot_auth.awaiting_snapshot {
                        "Usage Radar is waiting for GitHub sign-in and the first Copilot usage refresh to finish."
                            .to_string()
                    } else if self.copilot_auth.has_saved_token {
                        "Usage Radar found a saved GitHub sign-in and is waiting for Copilot usage to refresh."
                            .to_string()
                    } else {
                        "Use the Copilot page to sign in with GitHub before Usage Radar reads Copilot usage."
                            .to_string()
                    }),
                },
                ProviderKind::OpenCodeGo => ProviderCardModel {
                    title,
                    accent,
                    subtitle: None,
                    sections: Vec::new(),
                    metrics: Vec::new(),
                    headline: Some(if self.refresh.in_flight {
                        "Looking for an OpenCode Go session".to_string()
                    } else {
                        "OpenCode Go not connected yet".to_string()
                    }),
                    detail: Some(if self.refresh.in_flight {
                        "Usage Radar is checking Chrome, Brave, and Edge first. If none has a usable session, you can use the manual Cookie fallback on the OpenCode Go page."
                            .to_string()
                    } else {
                        "Usage Radar will try to import your OpenCode Go browser session on Windows first. If that fails, you can paste a manual Cookie header on the OpenCode Go page."
                            .to_string()
                    }),
                },
                _ => ProviderCardModel {
                    title,
                    accent,
                    subtitle: None,
                    sections: Vec::new(),
                    metrics: Vec::new(),
                    headline: Some("Support not wired yet".to_string()),
                    detail: Some(
                        "This page stays visible, but Usage Radar will not invent data until a trustworthy source exists."
                            .to_string(),
                    ),
                },
            };
        };

        if snapshot.unavailable {
            let headline = if kind == ProviderKind::OpenCodeGo {
                "OpenCode Go not connected yet".to_string()
            } else {
                "Usage temporarily unavailable".to_string()
            };

            return ProviderCardModel {
                title,
                accent,
                subtitle: snapshot_subtitle(snapshot),
                sections: Vec::new(),
                metrics: Vec::new(),
                headline: Some(headline),
                detail: Some(first_meaningful_note(snapshot).unwrap_or_else(|| {
                    if kind == ProviderKind::OpenCodeGo {
                        "Usage Radar will try browser import first. If that does not work, use the manual Cookie fallback on the OpenCode Go page."
                            .to_string()
                    } else {
                        "The provider is expected, but no reliable snapshot is available yet."
                            .to_string()
                    }
                })),
            };
        }

        let sections = provider_sections(kind, snapshot);
        let metrics = provider_metrics(kind, snapshot);

        if sections.is_empty() && metrics.is_empty() {
            return ProviderCardModel {
                title,
                accent,
                subtitle: snapshot_subtitle(snapshot),
                sections,
                metrics,
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
            subtitle: snapshot_subtitle(snapshot),
            sections,
            metrics,
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

    fn codex_billing_credits_available(&self) -> bool {
        self.snapshot(ProviderKind::Codex)
            .and_then(|snapshot| snapshot.web_credits.as_ref())
            .and_then(|credits| credits.remaining)
            .is_some()
    }

    fn provider_plan_label(&self, kind: ProviderKind) -> Option<String> {
        self.snapshot(kind)
            .and_then(|snapshot| plan_label_from_notes(&snapshot.notes))
    }

    fn provider_enabled(&self, kind: ProviderKind) -> bool {
        !self.config.disabled_providers.contains(&kind)
    }

    fn enabled_refresh_providers(&self) -> Vec<ProviderKind> {
        TRACKABLE_PROVIDERS
            .into_iter()
            .filter(|kind| self.provider_enabled(*kind))
            .collect()
    }

    fn open_code_go_setup_visible(&self, snapshot: Option<&ProviderSnapshot>) -> bool {
        self.panel.show_open_code_go_setup
            || snapshot
                .map(|snapshot| snapshot.unavailable)
                .unwrap_or(!self.refresh.in_flight)
    }

    fn open_code_go_manual_override_active(&self) -> bool {
        self.config
            .opencode_go_cookie_header
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
    }

    fn open_code_go_workspace_override_active(&self) -> bool {
        self.config
            .opencode_go_workspace_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
    }

    fn open_code_go_connection_detail(
        &self,
        snapshot: Option<&ProviderSnapshot>,
        manual_override_active: bool,
    ) -> String {
        let workspace_override_active = self.open_code_go_workspace_override_active();

        if manual_override_active {
            if workspace_override_active {
                "Usage Radar is using the saved Cookie header and workspace override instead of browser auto import."
                    .to_string()
            } else {
                "Usage Radar is using the saved Cookie header instead of browser auto import."
                    .to_string()
            }
        } else if let Some(note) = snapshot.and_then(first_meaningful_note) {
            if workspace_override_active {
                format!("{note} Workspace override saved.")
            } else {
                note
            }
        } else if workspace_override_active {
            "Usage Radar is using browser import with a saved workspace override.".to_string()
        } else {
            "Usage Radar is using browser import for OpenCode Go.".to_string()
        }
    }

    fn should_show_copilot_connect_button(&self) -> bool {
        !self.copilot_auth.is_busy() && !self.copilot_auth.has_saved_token
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
    metrics: Vec<ProviderMetric>,
    headline: Option<String>,
    detail: Option<String>,
}

#[derive(Debug, Clone)]
struct ProviderSection {
    title: String,
    progress: f32,
    leading: String,
    trailing: Option<String>,
    accent: Color,
}

#[derive(Debug, Clone)]
struct ProviderMetric {
    value: String,
    unit: Option<String>,
    detail: Option<String>,
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
    OpenCode,
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

fn handle_panel_focus_event(
    event: Event,
    _status: event::Status,
    window: window::Id,
) -> Option<Message> {
    match event {
        Event::Window(window::Event::Focused) => Some(Message::PanelFocusChanged(window, true)),
        Event::Window(window::Event::Unfocused) => Some(Message::PanelFocusChanged(window, false)),
        _ => None,
    }
}

fn weighted_font(weight: font::Weight) -> Font {
    Font {
        weight,
        ..Font::DEFAULT
    }
}

#[cfg(target_os = "windows")]
fn spawn_open_target(target: &str) -> std::io::Result<std::process::Child> {
    Command::new("explorer").arg(target).spawn()
}

#[cfg(target_os = "macos")]
fn spawn_open_target(target: &str) -> std::io::Result<std::process::Child> {
    Command::new("open").arg(target).spawn()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_open_target(target: &str) -> std::io::Result<std::process::Child> {
    Command::new("xdg-open").arg(target).spawn()
}

fn chatgpt_billing_probe_path() -> Result<std::path::PathBuf, String> {
    let current = std::env::current_exe()
        .map_err(|error| format!("Could not locate Usage Radar executable: {error}"))?;
    let directory = current
        .parent()
        .ok_or_else(|| format!("Executable has no parent directory: {}", current.display()))?;
    let executable = if cfg!(target_os = "windows") {
        "chatgpt_billing_probe.exe"
    } else {
        "chatgpt_billing_probe"
    };
    let path = directory.join(executable);

    if path.exists() {
        Ok(path)
    } else {
        Err(format!(
            "ChatGPT billing balance helper was not found at {}. Build with `cargo build --bins`.",
            path.display()
        ))
    }
}

fn page_tab_button(
    label: &'static str,
    icon: TabIcon,
    active: bool,
    message: Message,
    _accent: Color,
) -> Element<'static, Message> {
    let active_content = Color::WHITE;
    let icon_color = if active {
        active_content
    } else {
        color_muted()
    };

    let content = column![
        container(tab_icon(icon, icon_color))
            .width(Length::Fill)
            .height(Length::Fixed(17.0))
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center),
        text(label)
            .size(12)
            .font(weighted_font(font::Weight::Medium))
            .color(if active {
                active_content
            } else {
                color_muted()
            }),
    ]
    .spacing(2)
    .align_x(alignment::Horizontal::Center)
    .width(Length::Fill);

    container(
        button(content)
            .width(Length::Fill)
            .height(Length::Fixed(42.0))
            .padding([5, 4])
            .style(move |_theme, status| page_tab_style(active, status))
            .on_press(message),
    )
    .width(Length::FillPortion(1))
    .height(Length::Fixed(42.0))
    .into()
}

fn tab_icon(icon: TabIcon, color: Color) -> Element<'static, Message> {
    let size = match icon {
        TabIcon::Codex => 17.0,
        TabIcon::OpenCode => 14.0,
        _ => 16.0,
    };

    svg::Svg::new(tab_icon_handle(icon))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_theme, _status| svg::Style { color: Some(color) })
        .into()
}

fn tab_icon_handle(icon: TabIcon) -> svg::Handle {
    match icon {
        TabIcon::Home => svg::Handle::from_memory(include_bytes!("../../assets/gauge.svg")),
        TabIcon::Codex => {
            svg::Handle::from_memory(include_bytes!("../../assets/provider-icon-codex.svg"))
        }
        TabIcon::Copilot => {
            svg::Handle::from_memory(include_bytes!("../../assets/githubcopilot.svg"))
        }
        TabIcon::OpenCode => {
            svg::Handle::from_memory(include_bytes!("../../assets/opencode-logo-dark.svg"))
        }
    }
}

fn provider_list_row(model: ProviderCardModel) -> Element<'static, Message> {
    let ProviderCardModel {
        title,
        accent: _,
        subtitle,
        sections,
        metrics,
        headline,
        detail,
    } = model;

    if !sections.is_empty() || !metrics.is_empty() {
        let mut body = column![row![
            text(title)
                .size(15)
                .font(weighted_font(font::Weight::Semibold))
                .color(color_text()),
            horizontal_space(),
            subtitle
                .map(|value| text(value).size(11).color(color_muted()))
                .unwrap_or_else(|| text("").size(1)),
        ]
        .align_y(Alignment::Center)]
        .spacing(8);

        for section in sections {
            body = body.push(provider_list_section(section));
        }

        for metric in metrics {
            body = body.push(provider_list_metric(metric));
        }

        return container(body).width(Length::Fill).padding([10, 4]).into();
    }

    let detail_text = detail
        .or(headline)
        .unwrap_or_else(|| "No usage snapshot yet.".to_string());

    container(
        column![
            text(title)
                .size(15)
                .font(weighted_font(font::Weight::Semibold))
                .color(color_text()),
            text(detail_text).size(11).color(color_muted()),
        ]
        .spacing(5),
    )
    .width(Length::Fill)
    .padding([10, 4])
    .into()
}

fn provider_list_section(section: ProviderSection) -> Element<'static, Message> {
    column![
        row![
            text(section.title)
                .size(12)
                .font(weighted_font(font::Weight::Semibold))
                .color(color_text()),
            horizontal_space(),
            text(
                section
                    .trailing
                    .unwrap_or_else(|| "Reset unknown".to_string())
            )
            .size(11)
            .color(color_muted()),
        ]
        .align_y(Alignment::Center),
        progress_bar(0.0..=100.0, section.progress)
            .height(6)
            .style(move |_theme| progress_style(section.accent)),
        text(section.leading).size(11).color(color_muted()),
    ]
    .spacing(5)
    .into()
}

fn provider_panel(
    model: ProviderCardModel,
    framed: bool,
    show_title: bool,
) -> Element<'static, Message> {
    let accent = model.accent;
    let body = provider_panel_body(model, show_title);

    if framed {
        container(body)
            .width(Length::Fill)
            .padding([10, 4])
            .style(move |_theme| provider_card_style(accent))
            .into()
    } else {
        container(body).width(Length::Fill).padding([1, 1]).into()
    }
}

fn provider_panel_body(
    model: ProviderCardModel,
    show_title: bool,
) -> iced::widget::Column<'static, Message> {
    let mut body = column!().spacing(12);

    if show_title {
        body = body.push(
            text(model.title)
                .size(18)
                .font(weighted_font(font::Weight::Bold))
                .color(color_text()),
        );
    }

    if let Some(subtitle) = model.subtitle {
        body = body.push(text(subtitle).size(11).color(color_muted()));
    }

    if let Some(headline) = model.headline {
        body = body.push(text(headline).size(13).color(color_text()));
    }

    if let Some(detail) = model.detail {
        body = body.push(text(detail).size(11).color(color_muted()));
    }

    for section in model.sections {
        body = body.push(provider_section(section));
    }

    for metric in model.metrics {
        body = body.push(provider_metric(metric));
    }

    body
}

fn provider_section(section: ProviderSection) -> Element<'static, Message> {
    let mut title_row = row![text(section.title)
        .size(14)
        .font(weighted_font(font::Weight::Semibold))
        .color(color_text())]
    .align_y(Alignment::Center)
    .width(Length::Fill);

    if let Some(trailing) = section.trailing {
        title_row = title_row
            .push(horizontal_space())
            .push(text(trailing).size(12).color(color_muted()));
    }

    column![
        title_row,
        progress_bar(0.0..=100.0, section.progress)
            .height(7)
            .style(move |_theme| progress_style(section.accent)),
        text(section.leading).size(12).color(color_text()),
    ]
    .spacing(7)
    .into()
}

fn provider_list_metric(metric: ProviderMetric) -> Element<'static, Message> {
    let value = credit_metric_value(metric.value, metric.unit, metric.accent, false);

    let mut value_row = row![value].align_y(Alignment::Center);

    if let Some(detail) = metric.detail {
        column![value_row, text(detail).size(11).color(color_muted())]
            .spacing(4)
            .into()
    } else {
        value_row = value_row.width(Length::Fill);
        column![value_row].into()
    }
}

fn provider_metric(metric: ProviderMetric) -> Element<'static, Message> {
    let value = credit_metric_value(metric.value, metric.unit, metric.accent, true);

    let mut body = column![row![value].align_y(Alignment::Center)].spacing(5);

    if let Some(detail) = metric.detail {
        body = body.push(text(detail).size(11).color(color_muted()));
    }

    body.into()
}

fn credit_metric_value(
    value: String,
    unit: Option<String>,
    accent: Color,
    prominent: bool,
) -> Element<'static, Message> {
    let value_size = if prominent { 18 } else { 15 };
    let unit_size = if prominent { 13 } else { 12 };
    let mut content = row![text(value)
        .size(value_size)
        .font(weighted_font(font::Weight::Semibold))
        .color(accent)]
    .spacing(5)
    .align_y(Alignment::End);

    if let Some(unit) = unit {
        content = content.push(text(unit).size(unit_size).color(color_muted()));
    }

    content.into()
}

fn copilot_page_header(has_saved_token: bool) -> Element<'static, Message> {
    let mut header = row![
        column![
            text("Copilot")
                .size(17)
                .font(weighted_font(font::Weight::Bold))
                .color(color_text()),
            text("Updated just now").size(12).color(color_muted()),
        ]
        .spacing(2),
        horizontal_space(),
        text("Partial").size(13).color(color_muted()),
    ]
    .align_y(Alignment::Center)
    .width(Length::Fill);

    if has_saved_token {
        header = header.push(inline_action_button(
            LucideIcon::LogOut,
            "Sign out",
            Message::CopilotSignOutRequested,
        ));
    }

    column![header, divider_line()].spacing(8).into()
}

fn open_code_go_page_header() -> Element<'static, Message> {
    provider_page_header("OpenCode Go", None)
}

fn provider_page_header(
    title: &'static str,
    trailing: Option<String>,
) -> Element<'static, Message> {
    let mut header = row![
        column![
            text(title)
                .size(17)
                .font(weighted_font(font::Weight::Bold))
                .color(color_text()),
            text("Updated just now").size(12).color(color_muted()),
        ]
        .spacing(2),
        horizontal_space(),
    ]
    .align_y(Alignment::Center)
    .width(Length::Fill);

    if let Some(trailing) = trailing {
        header = header.push(text(trailing).size(13).color(color_muted()));
    }

    column![header, divider_line()].spacing(10).into()
}

fn back_page_header(
    title: &'static str,
    subtitle: Option<&'static str>,
    trailing: Option<&'static str>,
) -> Element<'static, Message> {
    let mut title_column = column![text(title)
        .size(18)
        .font(weighted_font(font::Weight::Bold))
        .color(color_text())]
    .spacing(2);

    if let Some(subtitle) = subtitle {
        title_column = title_column.push(text(subtitle).size(12).color(color_muted()));
    }

    let back = text_icon_button(LucideIcon::ChevronLeft, "Back", Message::BackToMain);
    let title_row = row![
        title_column,
        horizontal_space(),
        trailing
            .map(|value| text(value).size(13).color(color_muted()))
            .unwrap_or_else(|| text("").size(1)),
    ]
    .align_y(Alignment::Center)
    .width(Length::Fill);

    column![back, title_row, divider_line()].spacing(7).into()
}

fn settings_card(content: iced::widget::Column<'static, Message>) -> Element<'static, Message> {
    container(content.spacing(7))
        .width(Length::Fill)
        .padding([10, 11])
        .style(|_theme| settings_group_style())
        .into()
}

fn setting_toggle_row(
    title: &'static str,
    detail: &'static str,
    enabled: bool,
    message: Message,
) -> Element<'static, Message> {
    row![
        column![
            text(title).size(13).color(color_text()),
            text(detail).size(10).color(color_muted()),
        ]
        .spacing(3)
        .width(Length::Fill),
        setting_toggle_button(enabled, message),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

fn settings_menu_row(
    icon: LucideIcon,
    label: &'static str,
    message: Message,
) -> Element<'static, Message> {
    button(
        row![
            text(char::from(icon).to_string())
                .font(Font::with_name("lucide"))
                .size(15)
                .color(color_text()),
            text(label).size(13).color(color_text()),
            horizontal_space(),
            text(char::from(LucideIcon::ChevronRight).to_string())
                .font(Font::with_name("lucide"))
                .size(14)
                .color(color_muted()),
        ]
        .spacing(9)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([4, 0])
    .style(inline_action_button_style)
    .on_press(message)
    .into()
}

fn setting_choice_button(
    label: &'static str,
    active: bool,
    message: Message,
) -> iced::widget::Button<'static, Message> {
    button(
        text(label)
            .size(11)
            .color(if active { color_text() } else { color_muted() }),
    )
    .padding([7, 10])
    .style(move |_theme, status| setting_choice_button_style(active, status))
    .on_press(message)
}

fn setting_toggle_button(
    enabled: bool,
    message: Message,
) -> iced::widget::Button<'static, Message> {
    let knob = container(text(""))
        .width(Length::Fixed(16.0))
        .height(Length::Fixed(16.0))
        .style(|_theme| toggle_knob_style());
    let content = if enabled {
        row![horizontal_space(), knob]
    } else {
        row![knob, horizontal_space()]
    };

    button(
        container(content.align_y(Alignment::Center))
            .width(Length::Fixed(38.0))
            .height(Length::Fixed(22.0))
            .padding(3)
            .style(move |_theme| setting_toggle_style(enabled)),
    )
    .padding(0)
    .style(move |_theme, status| setting_toggle_button_style(enabled, status))
    .on_press(message)
}

fn action_status_card(title: &'static str, detail: &str) -> Element<'static, Message> {
    container(
        column![
            text(title).size(13).color(color_text()),
            text(detail.to_string()).size(11).color(color_muted()),
        ]
        .spacing(5),
    )
    .width(Length::Fill)
    .padding(10)
    .style(|_theme| settings_group_style())
    .into()
}

fn chatgpt_billing_setup_card(can_collapse: bool) -> Element<'static, Message> {
    let action_label = if can_collapse {
        "Update OpenAI web balance"
    } else {
        "Connect OpenAI web"
    };

    let mut secondary_actions = row![text_inline_button(
        "Clear web session",
        Message::ClearCodexSettings
    )]
    .spacing(10)
    .align_y(Alignment::Center);

    if can_collapse {
        secondary_actions =
            secondary_actions.push(text_inline_button("Done", Message::HideCodexCookieSetup));
    }

    container(
        column![
            text("OpenAI web billing").size(13).color(color_text()),
            text("Connect ChatGPT billing once, then Usage Radar can refresh the web credit balance in the background.")
                .size(11)
                .color(color_muted()),
            primary_action_button(
                action_label,
                provider_accent(ProviderKind::Codex),
                Message::OpenChatGptBillingProbe,
            ),
            secondary_actions,
        ]
        .spacing(7),
    )
    .width(Length::Fill)
    .padding(10)
    .style(|_theme| provider_card_style(color_border()))
    .into()
}

fn copilot_waiting_card(
    prompt: &crate::providers::copilot::DeviceCodePrompt,
) -> Element<'static, Message> {
    container(
        column![
            text("Finish GitHub sign-in").size(13).color(color_text()),
            text("A browser tab should already be open. If not, open GitHub manually below and finish approval.")
                .size(11)
                .color(color_muted()),
            row![
                container(text(prompt.user_code.clone()).size(18).color(color_text()))
                    .padding([9, 11])
                    .style(|_theme| iced::widget::container::Style {
                        background: Some(surface_shell().into()),
                        border: Border {
                            width: 1.0,
                            radius: 10.0.into(),
                            color: color_border(),
                        },
                        ..Default::default()
                    }),
                code_copy_button(Message::CopyCopilotCode),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
            text(prompt.verification_uri.clone())
                .size(11)
                .color(color_muted()),
            primary_action_button(
                "Open GitHub",
                provider_accent(ProviderKind::Copilot),
                Message::OpenCopilotVerification,
            ),
        ]
        .spacing(8),
    )
    .width(Length::Fill)
    .padding(10)
    .style(|_theme| provider_card_style(color_border()))
    .into()
}

fn primary_action_button(
    label: &'static str,
    accent: Color,
    message: Message,
) -> iced::widget::Button<'static, Message> {
    button(text(label).size(12).color(color_text()))
        .width(Length::Fill)
        .padding([9, 10])
        .style(move |_theme, status| primary_action_button_style(accent, status))
        .on_press(message)
}

fn open_code_go_connection_card(
    title: &'static str,
    detail: String,
    manual_override_active: bool,
) -> Element<'static, Message> {
    let mut actions = row![text_inline_button(
        "Edit setup",
        Message::ShowOpenCodeGoSetup
    )]
    .spacing(10)
    .align_y(Alignment::Center);

    actions = if manual_override_active {
        actions.push(text_inline_button(
            "Clear override",
            Message::ClearOpenCodeGoSettings,
        ))
    } else {
        actions.push(text_inline_button(
            "Open OpenCode Go",
            Message::OpenOpenCodeGo,
        ))
    };

    container(
        column![
            text(title).size(13).color(color_text()),
            text(detail).size(11).color(color_muted()),
            actions,
        ]
        .spacing(7),
    )
    .width(Length::Fill)
    .padding(10)
    .style(|_theme| settings_group_style())
    .into()
}

fn open_code_go_setup_card(
    cookie_header: &str,
    workspace_id: &str,
    can_collapse: bool,
) -> Element<'static, Message> {
    let mut secondary_actions = row![
        text_inline_button("Open OpenCode Go", Message::OpenOpenCodeGo),
        text_inline_button("Clear", Message::ClearOpenCodeGoSettings),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    if can_collapse {
        secondary_actions =
            secondary_actions.push(text_inline_button("Done", Message::HideOpenCodeGoSetup));
    }

    let actions = column![
        primary_action_button(
            "Save & refresh",
            provider_accent(ProviderKind::OpenCodeGo),
            Message::SaveOpenCodeGoSettings,
        ),
        secondary_actions,
    ]
    .spacing(8);

    container(
        column![
            text("Manual Cookie fallback").size(13).color(color_text()),
            text("Usage Radar will try to import your OpenCode Go session from Chrome, Brave, or Edge on Windows. Paste a Cookie header here only if that fails. Workspace ID is optional.")
                .size(11)
                .color(color_muted()),
            text("Cookie header").size(11).color(color_text()),
            text_input("auth=...; __Host-auth=...", cookie_header)
                .on_input(Message::OpenCodeGoCookieHeaderChanged)
                .padding([8, 10])
                .size(12),
            text("Workspace ID (optional)").size(11).color(color_text()),
            text_input("wrk_... or workspace URL", workspace_id)
                .on_input(Message::OpenCodeGoWorkspaceIdChanged)
                .padding([8, 10])
                .size(12),
            text("If this field is filled in, Usage Radar uses it instead of browser auto import. You can also set OPENCODE_GO_COOKIE_HEADER or edit config.json directly if you prefer.")
                .size(10)
                .color(color_muted()),
            actions,
        ]
        .spacing(7),
    )
    .width(Length::Fill)
    .padding(10)
    .style(|_theme| provider_card_style(color_border()))
    .into()
}

fn inline_action_button(
    icon: LucideIcon,
    label: &'static str,
    message: Message,
) -> iced::widget::Button<'static, Message> {
    button(
        row![
            text(char::from(icon).to_string())
                .font(Font::with_name("lucide"))
                .size(14),
            text(label).size(11),
        ]
        .spacing(5)
        .align_y(Alignment::Center),
    )
    .padding([3, 0])
    .style(inline_action_button_style)
    .on_press(message)
}

fn text_icon_button(
    icon: LucideIcon,
    label: &'static str,
    message: Message,
) -> iced::widget::Button<'static, Message> {
    button(
        row![
            text(char::from(icon).to_string())
                .font(Font::with_name("lucide"))
                .size(12),
            text(label).size(11),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .padding([2, 0])
    .style(inline_action_button_style)
    .on_press(message)
}

fn code_copy_button(message: Message) -> iced::widget::Button<'static, Message> {
    button(
        container(
            text(char::from(LucideIcon::Copy).to_string())
                .font(Font::with_name("lucide"))
                .size(15)
                .color(color_text()),
        )
        .width(Length::Fixed(38.0))
        .height(Length::Fixed(38.0))
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center),
    )
    .padding(0)
    .style(code_copy_button_style)
    .on_press(message)
}

fn text_inline_button(
    label: &'static str,
    message: Message,
) -> iced::widget::Button<'static, Message> {
    button(text(label).size(11))
        .padding([3, 0])
        .style(inline_action_button_style)
        .on_press(message)
}

fn toolbar_icon_button(
    icon: LucideIcon,
    message: Message,
) -> iced::widget::Button<'static, Message> {
    button(
        container(lucide_icon(icon))
            .width(Length::Fixed(34.0))
            .height(Length::Fixed(34.0))
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center),
    )
    .padding(0)
    .style(toolbar_icon_button_style)
    .on_press(message)
}

fn lucide_icon(icon: LucideIcon) -> Element<'static, Message> {
    text(char::from(icon).to_string())
        .font(Font::with_name("lucide"))
        .size(16)
        .color(color_text())
        .into()
}

fn notice_view(message: String, tone: Tone) -> Element<'static, Message> {
    let colors = tone_colors(tone);

    container(text(message).size(11).color(colors.text))
        .width(Length::Fill)
        .padding(10)
        .style(move |_theme| iced::widget::container::Style {
            background: Some(surface_card().into()),
            border: Border {
                width: 1.0,
                radius: 10.0.into(),
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
                leading: format_percent_left(bar.percent_left),
                trailing: format_reset_text(bar.reset_at),
                accent: remaining_accent(kind, bar.percent_left),
            })
            .collect()
    } else if let Some(bar) = snapshot.summary_bar.as_ref() {
        vec![ProviderSection {
            title: section_label(kind, &bar.label),
            progress: bar.percent_left.clamp(0.0, 100.0),
            leading: format_percent_left(bar.percent_left),
            trailing: format_reset_text(bar.reset_at),
            accent: remaining_accent(kind, bar.percent_left),
        }]
    } else {
        Vec::new()
    }
}

fn provider_metrics(kind: ProviderKind, snapshot: &ProviderSnapshot) -> Vec<ProviderMetric> {
    let mut metrics = Vec::new();

    if let Some(credits) = snapshot.credits.as_ref() {
        if let Some(metric) = provider_credit_metric(kind, snapshot, credits) {
            metrics.push(metric);
        }
    }

    if kind == ProviderKind::Codex {
        if let Some(credits) = snapshot.web_credits.as_ref() {
            if let Some(metric) = provider_credit_metric(kind, snapshot, credits) {
                metrics.push(metric);
            }
        }
    }

    metrics
}

fn provider_credit_metric(
    kind: ProviderKind,
    snapshot: &ProviderSnapshot,
    credits: &CreditBalance,
) -> Option<ProviderMetric> {
    let (value, unit) = if credits.unlimited {
        match credits.remaining {
            Some(remaining) => (
                format_credit_amount(remaining),
                Some("+ unlimited".to_string()),
            ),
            None => ("Unlimited".to_string(), None),
        }
    } else if let Some(remaining) = credits.remaining {
        (format_credit_amount(remaining), Some("credits".to_string()))
    } else {
        return None;
    };

    let detail = credits.scope.as_ref().map(|scope| {
        let clean_scope = scope
            .trim_start_matches("ChatGPT ")
            .trim_start_matches("Codex ")
            .to_string();
        let freshness = credit_freshness_label(credits);
        let detail = if let Some(freshness) = freshness {
            format!("{clean_scope} · {freshness}")
        } else {
            clean_scope
        };

        if snapshot.stale {
            format!("Last known {detail}")
        } else {
            detail
        }
    });

    Some(ProviderMetric {
        value,
        unit,
        detail,
        accent: provider_accent(kind),
    })
}

fn provider_ui_label(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Codex => "Codex",
        ProviderKind::Copilot => "Copilot",
        ProviderKind::ClaudeCode => "Claude",
        ProviderKind::OpenCodeGo => "OpenCode Go",
    }
}

fn provider_settings_detail(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Codex => "Reads local Codex auth and usage windows.",
        ProviderKind::Copilot => "Uses GitHub sign-in and Copilot quota data.",
        ProviderKind::OpenCodeGo => "Uses browser import or a manual cookie fallback.",
        ProviderKind::ClaudeCode => "Not wired yet.",
    }
}

fn refresh_interval_label(minutes: u64) -> &'static str {
    match minutes {
        0 => "Manual",
        1 => "1m",
        5 => "5m",
        15 => "15m",
        _ => "Custom",
    }
}

fn appearance_label(appearance: AppAppearance) -> &'static str {
    match appearance {
        AppAppearance::Light => "Light",
        AppAppearance::Dark => "Dark",
    }
}

fn section_label(kind: ProviderKind, label: &str) -> String {
    match (kind, label) {
        (ProviderKind::Codex, "5h window") => "Session".to_string(),
        (ProviderKind::Codex, "Weekly window") => "Weekly".to_string(),
        (_, "5h window") => "Current".to_string(),
        (_, "Weekly window") => "Weekly".to_string(),
        (_, "Monthly window") => "Monthly".to_string(),
        _ => label.trim_end_matches(" window").to_string(),
    }
}

const DISPLAY_NOTE_PREFIX: &str = "Display note:";
const TECHNICAL_DETAIL_PREFIX: &str = "Technical detail:";
const PLAN_NOTE_PREFIX: &str = "Plan:";

fn first_meaningful_note(snapshot: &ProviderSnapshot) -> Option<String> {
    snapshot.notes.iter().find_map(|note| {
        if let Some(display_note) = note.strip_prefix(DISPLAY_NOTE_PREFIX) {
            Some(display_note.trim_start().to_string())
        } else if note.starts_with(PLAN_NOTE_PREFIX) || note.starts_with(TECHNICAL_DETAIL_PREFIX) {
            None
        } else {
            Some(note.clone())
        }
    })
}

fn plan_label_from_notes(notes: &[String]) -> Option<String> {
    notes
        .iter()
        .find_map(|note| note.strip_prefix(PLAN_NOTE_PREFIX))
        .map(str::trim)
        .filter(|plan| !plan.is_empty())
        .map(format_plan_label)
}

fn format_plan_label(plan: &str) -> String {
    let normalized = plan
        .trim()
        .trim_start_matches("chatgpt_")
        .trim_start_matches("chatgpt-")
        .trim_start_matches("chatgpt ")
        .to_ascii_lowercase();

    match normalized.as_str() {
        "plus" => "Plus".to_string(),
        "pro" => "Pro".to_string(),
        "max" => "Max".to_string(),
        "business" | "self_serve_business" | "self_serve_business_usage_based" => {
            "Business".to_string()
        }
        "team" => "Team".to_string(),
        "enterprise" => "Enterprise".to_string(),
        "free" => "Free".to_string(),
        _ => plan
            .split(['_', '-', ' '])
            .filter(|word| !word.is_empty())
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => {
                        first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                    }
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn snapshot_subtitle(snapshot: &ProviderSnapshot) -> Option<String> {
    if snapshot.stale {
        return match snapshot.confidence {
            Confidence::Estimated => Some("Last known snapshot · Estimated".to_string()),
            _ => Some("Last known snapshot".to_string()),
        };
    }

    if snapshot.unavailable {
        return None;
    }

    match snapshot.confidence {
        Confidence::Estimated => Some("Estimated".to_string()),
        _ => None,
    }
}

fn provider_failure_snapshot(kind: ProviderKind, error: &str) -> ProviderSnapshot {
    ProviderSnapshot {
        kind,
        visible: true,
        confidence: Confidence::Partial,
        fetched_at: SystemTime::now(),
        stale: true,
        unavailable: true,
        summary_bar: None,
        detail_bars: Vec::new(),
        credits: None,
        web_credits: None,
        notes: vec![
            display_note(refresh_failure_message(kind, true)),
            technical_detail_note(error),
        ],
    }
}

fn refresh_failure_message(kind: ProviderKind, unavailable: bool) -> String {
    let provider = provider_ui_label(kind);

    if unavailable {
        format!("Couldn't load {provider} usage right now. Try again in a moment.")
    } else {
        format!("Couldn't refresh {provider} right now. Showing the last known snapshot.")
    }
}

fn display_note(note: String) -> String {
    format!("{DISPLAY_NOTE_PREFIX} {note}")
}

fn technical_detail_note(error: &str) -> String {
    format!("{TECHNICAL_DETAIL_PREFIX} {error}")
}

fn normalize_optional_config_value(value: Option<String>) -> Option<String> {
    let value = value?;
    let trimmed = value.trim();

    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn format_reset_text(reset_at: Option<SystemTime>) -> Option<String> {
    let reset_at = reset_at?;

    Some(match reset_at.duration_since(SystemTime::now()) {
        Ok(duration) if duration.as_secs() < 60 => "Resets in under 1m".to_string(),
        Ok(duration) if duration.as_secs() < 3_600 => {
            format!("Resets in {}m", duration.as_secs() / 60)
        }
        Ok(duration) if duration.as_secs() < 86_400 => {
            format!("Resets in {}h", duration.as_secs() / 3_600)
        }
        Ok(duration) => format!("Resets in {}d", duration.as_secs() / 86_400),
        Err(_) => "Reset pending".to_string(),
    })
}

fn format_percent_left(percent_left: f32) -> String {
    let rounded = (percent_left * 10.0).round() / 10.0;

    if (rounded - rounded.round()).abs() < 0.05 {
        format!("{rounded:.0}% left")
    } else {
        format!("{rounded:.1}% left")
    }
}

fn compact_age_since(time: SystemTime) -> Option<String> {
    let age = SystemTime::now().duration_since(time).ok()?;
    let seconds = age.as_secs();

    if seconds < 60 {
        Some("now".to_string())
    } else if seconds < 60 * 60 {
        Some(format!("{}m", seconds / 60))
    } else if seconds < 48 * 60 * 60 {
        Some(format!("{}h", seconds / 60 / 60))
    } else {
        Some(format!("{}d", seconds / 60 / 60 / 24))
    }
}

fn credit_freshness_label(credits: &CreditBalance) -> Option<String> {
    credits.captured_at.and_then(compact_age_since).map(|age| {
        if age == "now" {
            "updated now".to_string()
        } else {
            format!("updated {age} ago")
        }
    })
}

fn format_credit_amount(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;

    let raw = if (rounded - rounded.round()).abs() < 0.05 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    };

    add_number_grouping(&raw)
}

fn add_number_grouping(raw: &str) -> String {
    let (whole, fraction) = raw.split_once('.').unwrap_or((raw, ""));
    let mut grouped = String::new();

    for (index, character) in whole.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(character);
    }

    let mut value = grouped.chars().rev().collect::<String>();
    if !fraction.is_empty() {
        value.push('.');
        value.push_str(fraction);
    }
    value
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

pub(crate) fn set_current_appearance(appearance: AppAppearance) {
    CURRENT_APPEARANCE.store(appearance.as_u8(), Ordering::Relaxed);
}

fn current_appearance() -> AppAppearance {
    AppAppearance::from_u8(CURRENT_APPEARANCE.load(Ordering::Relaxed))
}

fn dark_mode_active() -> bool {
    current_appearance() == AppAppearance::Dark
}

fn panel_shell_style(_theme: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(surface_shell().into()),
        border: Border {
            width: 2.0,
            radius: 0.0.into(),
            color: panel_window_border(),
        },
        shadow: Shadow {
            color: Color::from_rgba8(20, 16, 42, 0.24),
            offset: iced::Vector::new(0.0, 10.0),
            blur_radius: 24.0,
        },
        ..Default::default()
    }
}

fn provider_card_style(_accent: Color) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Color::TRANSPARENT.into()),
        border: Border {
            width: 0.0,
            radius: 0.0.into(),
            color: Color::TRANSPARENT,
        },
        ..Default::default()
    }
}

fn settings_group_style() -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(surface_card().into()),
        border: Border {
            width: 1.0,
            radius: 14.0.into(),
            color: color_border(),
        },
        shadow: Shadow::default(),
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
                background = tab_hover_background();
                text_color = color_text();
            }
        }
        button::Status::Pressed => {
            if !active {
                background = tab_pressed_background();
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
            radius: 0.0.into(),
            color: Color::TRANSPARENT,
        },
        shadow: Shadow::default(),
    }
}

fn tab_hover_background() -> Color {
    if dark_mode_active() {
        Color::from_rgba8(132, 122, 158, 0.18)
    } else {
        Color::from_rgba8(255, 255, 255, 0.18)
    }
}

fn tab_pressed_background() -> Color {
    if dark_mode_active() {
        Color::from_rgba8(132, 122, 158, 0.24)
    } else {
        Color::from_rgba8(255, 255, 255, 0.26)
    }
}

fn toolbar_icon_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let (background, text_color) = match status {
        button::Status::Hovered => (Color::from_rgba8(255, 255, 255, 0.06), color_text()),
        button::Status::Pressed => (Color::from_rgba8(255, 255, 255, 0.10), color_text()),
        button::Status::Disabled => (Color::TRANSPARENT, Color::from_rgb8(120, 126, 134)),
        button::Status::Active => (Color::TRANSPARENT, color_text()),
    };

    button::Style {
        background: Some(background.into()),
        text_color,
        border: Border {
            width: 0.0,
            radius: 999.0.into(),
            color: Color::TRANSPARENT,
        },
        shadow: Shadow::default(),
    }
}

fn inline_action_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let text_color = match status {
        button::Status::Hovered | button::Status::Pressed => color_text(),
        button::Status::Disabled | button::Status::Active => color_muted(),
    };

    button::Style {
        background: Some(Color::TRANSPARENT.into()),
        text_color,
        border: Border {
            width: 0.0,
            radius: 6.0.into(),
            color: Color::TRANSPARENT,
        },
        shadow: Shadow::default(),
    }
}

fn setting_choice_button_style(active: bool, status: button::Status) -> button::Style {
    let mut background = if active {
        accent_home()
    } else {
        surface_shell()
    };
    let text_color = if active { color_text() } else { color_muted() };
    let mut border_color = if active {
        accent_home()
    } else {
        color_border()
    };

    match status {
        button::Status::Hovered => {
            if active {
                background = Color::from_rgba8(80, 128, 246, 1.0);
            } else {
                background = Color::from_rgba8(255, 255, 255, 0.06);
                border_color = Color::from_rgb8(97, 101, 108);
            }
        }
        button::Status::Pressed => {
            if !active {
                background = Color::from_rgba8(255, 255, 255, 0.10);
            }
        }
        button::Status::Disabled | button::Status::Active => {}
    }

    button::Style {
        background: Some(background.into()),
        text_color,
        border: Border {
            width: 1.0,
            radius: 9.0.into(),
            color: border_color,
        },
        shadow: Shadow::default(),
    }
}

fn setting_toggle_button_style(_enabled: bool, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => {
            if dark_mode_active() {
                Color::from_rgba8(255, 255, 255, 0.04)
            } else {
                Color::from_rgba8(255, 255, 255, 0.10)
            }
        }
        button::Status::Pressed => {
            if dark_mode_active() {
                Color::from_rgba8(255, 255, 255, 0.07)
            } else {
                Color::from_rgba8(255, 255, 255, 0.14)
            }
        }
        button::Status::Disabled | button::Status::Active => Color::TRANSPARENT,
    };

    button::Style {
        background: Some(background.into()),
        text_color: color_text(),
        border: Border {
            width: 0.0,
            radius: 999.0.into(),
            color: Color::TRANSPARENT,
        },
        shadow: Shadow::default(),
    }
}

fn setting_toggle_style(enabled: bool) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(
            if enabled {
                accent_home()
            } else {
                Color::from_rgba8(112, 104, 130, 0.34)
            }
            .into(),
        ),
        border: Border {
            width: 1.0,
            radius: 999.0.into(),
            color: if enabled {
                Color::from_rgba8(47, 121, 246, 0.82)
            } else {
                Color::from_rgba8(94, 87, 111, 0.38)
            },
        },
        shadow: Shadow::default(),
        ..Default::default()
    }
}

fn toggle_knob_style() -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Color::from_rgba8(255, 255, 255, 0.92).into()),
        border: Border {
            width: 0.0,
            radius: 999.0.into(),
            color: Color::TRANSPARENT,
        },
        shadow: Shadow {
            color: Color::from_rgba8(68, 56, 92, 0.22),
            offset: iced::Vector::new(0.0, 1.0),
            blur_radius: 2.0,
        },
        ..Default::default()
    }
}

fn code_copy_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => Color::from_rgba8(255, 255, 255, 0.06),
        button::Status::Pressed => Color::from_rgba8(255, 255, 255, 0.10),
        button::Status::Disabled | button::Status::Active => surface_shell(),
    };

    button::Style {
        background: Some(background.into()),
        text_color: color_text(),
        border: Border {
            width: 1.0,
            radius: 10.0.into(),
            color: color_border(),
        },
        shadow: Shadow::default(),
    }
}

fn primary_action_button_style(accent: Color, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => Color { a: 1.0, ..accent },
        button::Status::Pressed => Color::from_rgba(accent.r, accent.g, accent.b, 0.86),
        button::Status::Disabled => Color::from_rgba8(255, 255, 255, 0.08),
        button::Status::Active => accent,
    };

    button::Style {
        background: Some(background.into()),
        text_color: color_text(),
        border: Border {
            width: 0.0,
            radius: 10.0.into(),
            color: Color::TRANSPARENT,
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
        ProviderKind::Codex => color_rgb(47, 121, 246),
        ProviderKind::Copilot => color_rgb(62, 170, 142),
        ProviderKind::ClaudeCode => color_rgb(176, 131, 71),
        ProviderKind::OpenCodeGo => color_rgb(116, 128, 154),
    }
}

fn progress_accent(kind: ProviderKind) -> Color {
    provider_accent(kind)
}

fn remaining_accent(kind: ProviderKind, percent_left: f32) -> Color {
    if percent_left <= 5.0 {
        color_rgb(210, 113, 75)
    } else if percent_left <= 15.0 {
        color_rgb(211, 139, 83)
    } else {
        progress_accent(kind)
    }
}

fn color_text() -> Color {
    if dark_mode_active() {
        color_rgb(238, 235, 247)
    } else {
        color_rgb(37, 34, 46)
    }
}

fn color_muted() -> Color {
    if dark_mode_active() {
        color_rgb(167, 160, 188)
    } else {
        color_rgb(104, 98, 120)
    }
}

fn color_border() -> Color {
    if dark_mode_active() {
        Color::from_rgba8(220, 210, 255, 0.10)
    } else {
        Color::from_rgba8(118, 107, 145, 0.22)
    }
}

fn panel_window_border() -> Color {
    if dark_mode_active() {
        Color::from_rgba8(166, 158, 190, 0.42)
    } else {
        Color::from_rgba8(53, 45, 83, 0.72)
    }
}

fn color_divider() -> Color {
    if dark_mode_active() {
        Color::from_rgba8(220, 210, 255, 0.13)
    } else {
        Color::from_rgba8(101, 91, 126, 0.20)
    }
}

fn color_progress_track() -> Color {
    if dark_mode_active() {
        Color::from_rgba8(220, 210, 255, 0.12)
    } else {
        Color::from_rgba8(120, 110, 148, 0.18)
    }
}

fn color_warning_text() -> Color {
    if dark_mode_active() {
        color_rgb(242, 190, 137)
    } else {
        color_rgb(104, 65, 45)
    }
}

fn color_warning_border() -> Color {
    if dark_mode_active() {
        Color::from_rgba8(242, 190, 137, 0.28)
    } else {
        Color::from_rgba8(158, 98, 59, 0.32)
    }
}

fn surface_shell() -> Color {
    if dark_mode_active() {
        color_rgb(36, 33, 46)
    } else {
        color_rgb(219, 214, 250)
    }
}

fn surface_card() -> Color {
    if dark_mode_active() {
        Color::from_rgba8(255, 255, 255, 0.035)
    } else {
        Color::from_rgba8(244, 241, 255, 0.34)
    }
}

fn color_rgb(red: u8, green: u8, blue: u8) -> Color {
    Color::from_rgb8(red, green, blue)
}
