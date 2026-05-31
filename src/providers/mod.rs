pub mod codex;
pub mod copilot;
mod kinds;
pub mod opencode_go;
mod snapshot;

pub use kinds::ProviderKind;
pub use snapshot::{Confidence, LimitBar, ProviderSnapshot};

#[derive(Debug, Clone)]
pub struct RefreshOutcome {
    pub kind: ProviderKind,
    pub result: Result<ProviderSnapshot, String>,
}

pub async fn refresh_selected(providers: Vec<ProviderKind>) -> Vec<RefreshOutcome> {
    let mut outcomes = Vec::new();

    for kind in providers {
        let result = match kind {
            ProviderKind::Codex => codex::fetch_snapshot().await,
            ProviderKind::Copilot => copilot::fetch_snapshot().await,
            ProviderKind::OpenCodeGo => opencode_go::fetch_snapshot().await,
            ProviderKind::ClaudeCode => continue,
        };

        outcomes.push(RefreshOutcome { kind, result });
    }

    outcomes
}
