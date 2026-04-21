use iced::window;

use crate::app::state::RefreshReason;
use crate::providers::copilot::DeviceCodePrompt;
use crate::providers::{ProviderKind, RefreshOutcome};

#[derive(Debug, Clone)]
pub enum Message {
    AppStarted,
    Tick,
    PanelScrolled,
    StartPanelDrag,
    PanelFocusChanged(window::Id, bool),
    SelectPage(Option<ProviderKind>),
    OpenAbout,
    OpenConfigFolder,
    OpenOpenCodeGo,
    ShowOpenCodeGoSetup,
    HideOpenCodeGoSetup,
    OpenCodeGoCookieHeaderChanged(String),
    OpenCodeGoWorkspaceIdChanged(String),
    SaveOpenCodeGoSettings,
    ClearOpenCodeGoSettings,
    OpenCopilotVerification,
    CopyCopilotCode,
    CopilotConnectRequested,
    CopilotSignOutRequested,
    CopilotDeviceCodeReceived(u64, Result<DeviceCodePrompt, String>),
    CopilotSignInFinished(u64, Result<(), String>),
    RefreshRequested(RefreshReason),
    RefreshFinished(Vec<RefreshOutcome>),
    QuitRequested,
    EscapePressed(window::Id),
    PanelOpened(window::Id),
    PanelScaleFactorLoaded(window::Id, f32),
    PanelCloseRequested(window::Id),
    PanelClosed(window::Id),
}
