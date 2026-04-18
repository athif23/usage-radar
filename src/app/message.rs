use iced::window;

use crate::app::state::RefreshReason;
use crate::providers::ProviderSnapshot;

#[derive(Debug, Clone)]
pub enum Message {
    AppStarted,
    Tick,
    RefreshRequested(RefreshReason),
    RefreshFinished(Result<Vec<ProviderSnapshot>, String>),
    HidePanel,
    EscapePressed(window::Id),
    PanelOpened(window::Id),
    PanelScaleFactorLoaded(window::Id, f32),
    PanelCloseRequested(window::Id),
    PanelClosed(window::Id),
}
