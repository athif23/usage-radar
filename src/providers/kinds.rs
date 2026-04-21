use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Codex,
    Copilot,
    ClaudeCode,
    #[serde(rename = "opencode_go", alias = "gemini_cli")]
    OpenCodeGo,
}
