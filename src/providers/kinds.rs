use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Codex,
    Copilot,
    ClaudeCode,
    GeminiCli,
}

impl ProviderKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Copilot => "GitHub Copilot",
            Self::ClaudeCode => "Claude Code",
            Self::GeminiCli => "Gemini CLI",
        }
    }
}
