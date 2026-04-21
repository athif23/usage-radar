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

pub async fn refresh_all() -> Vec<RefreshOutcome> {
    vec![
        RefreshOutcome {
            kind: ProviderKind::Codex,
            result: codex::fetch_snapshot().await,
        },
        RefreshOutcome {
            kind: ProviderKind::Copilot,
            result: copilot::fetch_snapshot().await,
        },
        RefreshOutcome {
            kind: ProviderKind::OpenCodeGo,
            result: opencode_go::fetch_snapshot().await,
        },
    ]
}
