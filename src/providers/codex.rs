use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::providers::snapshot::{Confidence, LimitBar};
use crate::providers::{ProviderKind, ProviderSnapshot};

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

pub async fn fetch_snapshot() -> Result<ProviderSnapshot, String> {
    let auth = load_auth()?;
    let token = auth
        .tokens
        .access_token
        .ok_or_else(|| "Codex auth.json does not contain an access token".to_string())?;

    let response = reqwest::Client::new()
        .get(USAGE_URL)
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, "usage-radar")
        .send()
        .await
        .map_err(|error| format!("Could not reach Codex usage endpoint: {error}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Codex usage endpoint returned {}",
            response.status()
        ));
    }

    let usage: WhamUsage = response
        .json()
        .await
        .map_err(|error| format!("Could not decode Codex usage response: {error}"))?;

    let mut detail_bars = Vec::new();

    if let Some(window) = usage.rate_limit.primary_window {
        detail_bars.push(window.into_bar("5h window"));
    }

    if let Some(window) = usage.rate_limit.secondary_window {
        detail_bars.push(window.into_bar("Weekly window"));
    }

    if detail_bars.is_empty() {
        return Err("Codex usage response did not include any limit windows".to_string());
    }

    let summary_bar = detail_bars
        .iter()
        .cloned()
        .min_by(|left, right| left.percent_left.total_cmp(&right.percent_left));

    let mut notes = Vec::new();
    if let Some(plan_type) = usage.plan_type {
        notes.push(format!("Plan: {plan_type}"));
    }

    if let Some(credits) = usage.credits {
        if credits.has_credits {
            notes.push(format!("Credits balance: {}", credits.balance));
        }
    }

    Ok(ProviderSnapshot {
        kind: ProviderKind::Codex,
        visible: true,
        confidence: Confidence::Exact,
        fetched_at: SystemTime::now(),
        stale: false,
        unavailable: false,
        summary_bar,
        detail_bars,
        notes,
    })
}

fn load_auth() -> Result<CodexAuthFile, String> {
    let path = auth_file_path();
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read Codex auth file {}: {error}", path.display()))?;

    serde_json::from_str(&contents).map_err(|error| {
        format!(
            "Failed to parse Codex auth file {}: {error}",
            path.display()
        )
    })
}

fn auth_file_path() -> PathBuf {
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        PathBuf::from(codex_home).join("auth.json")
    } else {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".codex")
            .join("auth.json")
    }
}

#[derive(Debug, Deserialize)]
struct CodexAuthFile {
    tokens: CodexTokens,
}

#[derive(Debug, Deserialize)]
struct CodexTokens {
    access_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WhamUsage {
    plan_type: Option<String>,
    rate_limit: RateLimit,
    credits: Option<Credits>,
}

#[derive(Debug, Deserialize)]
struct RateLimit {
    primary_window: Option<UsageWindow>,
    secondary_window: Option<UsageWindow>,
}

#[derive(Debug, Deserialize)]
struct UsageWindow {
    used_percent: f32,
    reset_at: u64,
    reset_after_seconds: Option<u64>,
    limit_window_seconds: u64,
}

impl UsageWindow {
    fn into_bar(self, fallback_label: &str) -> LimitBar {
        let percent_used = self.used_percent.clamp(0.0, 100.0);
        let percent_left = (100.0 - percent_used).clamp(0.0, 100.0);
        let reset_at = UNIX_EPOCH.checked_add(Duration::from_secs(self.reset_at));

        LimitBar {
            label: match self.limit_window_seconds {
                18_000 => "5h window".to_string(),
                604_800 => "Weekly window".to_string(),
                _ => fallback_label.to_string(),
            },
            percent_used,
            percent_left,
            reset_at,
            subtitle: self
                .reset_after_seconds
                .map(|seconds| format!("Resets in {}", human_duration(seconds))),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Credits {
    balance: String,
    has_credits: bool,
}

fn human_duration(seconds: u64) -> String {
    if seconds < 60 {
        "under 1m".to_string()
    } else if seconds < 3_600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86_400 {
        format!("{}h", seconds / 3_600)
    } else {
        format!("{}d", seconds / 86_400)
    }
}
