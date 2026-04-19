use iced::window;

use crate::app::state::RefreshReason;
use crate::providers::{ProviderKind, ProviderSnapshot};

#[derive(Debug, Clone)]
pub enum Message {
    AppStarted,
    Tick,
    PanelScrolled,
    SelectProvider(ProviderKind),
    OpenConfigFolder,
    RefreshRequested(RefreshReason),
    RefreshFinished(Result<Vec<ProviderSnapshot>, String>),
    HidePanel,
    EscapePressed(window::Id),
    PanelOpened(window::Id),
    PanelScaleFactorLoaded(window::Id, f32),
    PanelCloseRequested(window::Id),
    PanelClosed(window::Id),
}
