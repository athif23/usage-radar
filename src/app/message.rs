use iced::window;

use crate::app::state::RefreshReason;
use crate::providers::copilot::DeviceCodePrompt;
use crate::providers::{ProviderKind, RefreshOutcome};

#[derive(Debug, Clone)]
pub enum Message {
    AppStarted,
    Tick,
    PanelScrolled,
    SelectPage(Option<ProviderKind>),
    OpenAbout,
    OpenConfigFolder,
    OpenCopilotVerification,
    CopilotConnectRequested,
    CopilotSignOutRequested,
    CopilotDeviceCodeReceived(Result<DeviceCodePrompt, String>),
    CopilotSignInFinished(Result<(), String>),
    RefreshRequested(RefreshReason),
    RefreshFinished(Vec<RefreshOutcome>),
    QuitRequested,
    EscapePressed(window::Id),
    PanelOpened(window::Id),
    PanelScaleFactorLoaded(window::Id, f32),
    PanelCloseRequested(window::Id),
    PanelClosed(window::Id),
}
