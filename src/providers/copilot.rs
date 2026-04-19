use std::time::{Duration, Instant, SystemTime};

use keyring::Entry;
use serde::Deserialize;

use crate::providers::{Confidence, LimitBar, ProviderKind, ProviderSnapshot};

const CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const ACCESS_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const USAGE_URL: &str = "https://api.github.com/copilot_internal/user";
const KEYRING_SERVICE: &str = "UsageRadar";
const KEYRING_ACCOUNT: &str = "copilot-github-token";
const EDITOR_VERSION: &str = "vscode/1.96.2";
const EDITOR_PLUGIN_VERSION: &str = "copilot-chat/0.26.7";
const USER_AGENT: &str = "GitHubCopilotChat/0.26.7";
const GITHUB_API_VERSION: &str = "2025-04-01";

#[derive(Debug, Clone)]
pub struct DeviceCodePrompt {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_url: String,
    pub interval_seconds: u64,
    pub expires_in_seconds: u64,
}

pub async fn fetch_snapshot() -> Result<ProviderSnapshot, String> {
    let Some(token) = load_token()? else {
        return Ok(disconnected_snapshot(
            "Sign in with GitHub to connect Copilot.",
        ));
    };

    let response = reqwest::Client::new()
        .get(USAGE_URL)
        .header(reqwest::header::AUTHORIZATION, format!("token {token}"))
        .header(reqwest::header::ACCEPT, "application/json")
        .header("Editor-Version", EDITOR_VERSION)
        .header("Editor-Plugin-Version", EDITOR_PLUGIN_VERSION)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header("X-Github-Api-Version", GITHUB_API_VERSION)
        .send()
        .await
        .map_err(|error| format!("Could not reach the Copilot usage endpoint: {error}"))?;

    match response.status() {
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            return Ok(disconnected_snapshot(
                "GitHub sign-in expired. Sign in again to reconnect Copilot.",
            ));
        }
        status if !status.is_success() => {
            return Err(format!("Copilot usage endpoint returned {status}"));
        }
        _ => {}
    }

    let usage: CopilotUsageResponse = response
        .json()
        .await
        .map_err(|error| format!("Could not decode Copilot usage response: {error}"))?;

    let mut detail_bars = Vec::new();

    if let Some(quota_snapshots) = usage.quota_snapshots {
        if let Some(bar) = quota_bar("Premium interactions", quota_snapshots.premium_interactions) {
            detail_bars.push(bar);
        }

        if let Some(bar) = quota_bar("Chat messages", quota_snapshots.chat) {
            detail_bars.push(bar);
        }

        if let Some(bar) = quota_bar("Code completions", quota_snapshots.completions) {
            detail_bars.push(bar);
        }
    }

    if detail_bars.is_empty() {
        if let (Some(remaining), Some(monthly)) = (usage.limited_user_quotas, usage.monthly_quotas)
        {
            if let Some(bar) = free_tier_bar("Chat messages", remaining.chat, monthly.chat) {
                detail_bars.push(bar);
            }

            if let Some(bar) = free_tier_bar(
                "Code completions",
                remaining.completions,
                monthly.completions,
            ) {
                detail_bars.push(bar);
            }
        }
    }

    if detail_bars.is_empty() {
        return Ok(disconnected_snapshot(
            "GitHub returned no Copilot quota data for this account.",
        ));
    }

    let summary_bar = detail_bars
        .iter()
        .cloned()
        .min_by(|left, right| left.percent_left.total_cmp(&right.percent_left));

    let mut notes = Vec::new();
    if !usage.copilot_plan.trim().is_empty() {
        notes.push(format!("Plan: {}", usage.copilot_plan));
    }
    if let Some(reset_date) = usage.quota_reset_date.or(usage.limited_user_reset_date) {
        notes.push(format!("Cycle reset: {reset_date}"));
    } else {
        notes.push("Reset timing unavailable from GitHub.".to_string());
    }

    Ok(ProviderSnapshot {
        kind: ProviderKind::Copilot,
        visible: true,
        confidence: Confidence::Partial,
        fetched_at: SystemTime::now(),
        stale: false,
        unavailable: false,
        summary_bar,
        detail_bars,
        notes,
    })
}

pub async fn request_device_code() -> Result<DeviceCodePrompt, String> {
    let response = reqwest::Client::new()
        .post(DEVICE_CODE_URL)
        .header(reqwest::header::ACCEPT, "application/json")
        .form(&[("client_id", CLIENT_ID), ("scope", "read:user")])
        .send()
        .await
        .map_err(|error| format!("Could not start GitHub device sign-in: {error}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "GitHub device sign-in returned {}",
            response.status()
        ));
    }

    let device_code: DeviceCodeResponse = response
        .json()
        .await
        .map_err(|error| format!("Could not decode GitHub device sign-in response: {error}"))?;

    Ok(DeviceCodePrompt {
        device_code: device_code.device_code,
        user_code: device_code.user_code,
        verification_uri: device_code.verification_uri.clone(),
        verification_url: device_code
            .verification_uri_complete
            .unwrap_or(device_code.verification_uri),
        interval_seconds: device_code.interval.max(1),
        expires_in_seconds: device_code.expires_in.max(60),
    })
}

pub async fn poll_for_token(prompt: &DeviceCodePrompt) -> Result<String, String> {
    let client = reqwest::Client::new();
    let started_at = Instant::now();
    let mut interval_seconds = prompt.interval_seconds.max(1);

    loop {
        if started_at.elapsed() >= Duration::from_secs(prompt.expires_in_seconds) {
            return Err("GitHub device code expired. Start sign-in again.".to_string());
        }

        tokio::time::sleep(Duration::from_secs(interval_seconds)).await;

        let response = client
            .post(ACCESS_TOKEN_URL)
            .header(reqwest::header::ACCEPT, "application/json")
            .form(&[
                ("client_id", CLIENT_ID),
                ("device_code", prompt.device_code.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await
            .map_err(|error| format!("Could not finish GitHub sign-in: {error}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "GitHub token exchange returned {}",
                response.status()
            ));
        }

        let body = response
            .text()
            .await
            .map_err(|error| format!("Could not read GitHub token response: {error}"))?;

        if let Ok(token_response) = serde_json::from_str::<AccessTokenResponse>(&body) {
            if !token_response.access_token.trim().is_empty() {
                return Ok(token_response.access_token);
            }
        }

        if let Ok(error_response) = serde_json::from_str::<DeviceFlowErrorResponse>(&body) {
            match error_response.error.as_str() {
                "authorization_pending" => continue,
                "slow_down" => {
                    interval_seconds += 5;
                    continue;
                }
                "expired_token" => {
                    return Err("GitHub device code expired. Start sign-in again.".to_string());
                }
                _ => {
                    return Err(error_response.error_description.unwrap_or_else(|| {
                        format!("GitHub sign-in failed: {}", error_response.error)
                    }));
                }
            }
        }

        return Err("GitHub returned an unexpected token response.".to_string());
    }
}

pub fn store_token(token: &str) -> Result<(), String> {
    credential_entry()?
        .set_password(token.trim())
        .map_err(|error| {
            format!("Failed to save Copilot sign-in to Windows credential storage: {error}")
        })
}

#[allow(dead_code)]
pub fn clear_token() -> Result<(), String> {
    let entry = credential_entry()?;

    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(format!(
            "Failed to clear Copilot sign-in from Windows credential storage: {error}"
        )),
    }
}

fn load_token() -> Result<Option<String>, String> {
    let entry = credential_entry()?;

    match entry.get_password() {
        Ok(token) => {
            let token = token.trim().to_string();
            if token.is_empty() {
                Ok(None)
            } else {
                Ok(Some(token))
            }
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!(
            "Failed to read Copilot sign-in from Windows credential storage: {error}"
        )),
    }
}

fn credential_entry() -> Result<Entry, String> {
    Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT).map_err(|error| {
        format!("Could not access Windows credential storage for Copilot: {error}")
    })
}

fn disconnected_snapshot(message: &str) -> ProviderSnapshot {
    ProviderSnapshot {
        kind: ProviderKind::Copilot,
        visible: true,
        confidence: Confidence::Partial,
        fetched_at: SystemTime::now(),
        stale: false,
        unavailable: true,
        summary_bar: None,
        detail_bars: Vec::new(),
        notes: vec![message.to_string()],
    }
}

fn quota_bar(label: &str, snapshot: Option<QuotaSnapshot>) -> Option<LimitBar> {
    let snapshot = snapshot?;
    if snapshot.unlimited.unwrap_or(false) {
        return None;
    }

    let percent_left = snapshot
        .percent_remaining
        .or_else(|| match (snapshot.remaining, snapshot.entitlement) {
            (Some(remaining), Some(entitlement)) if entitlement > 0 => {
                Some((remaining as f32 / entitlement as f32) * 100.0)
            }
            _ => None,
        })?
        .clamp(0.0, 100.0);

    let percent_used = (100.0 - percent_left).clamp(0.0, 100.0);
    let subtitle = match (snapshot.remaining, snapshot.entitlement) {
        (Some(remaining), Some(entitlement)) if entitlement > 0 => {
            Some(format!("{remaining} of {entitlement} left"))
        }
        _ => None,
    };

    Some(LimitBar {
        label: label.to_string(),
        percent_used,
        percent_left,
        reset_at: None,
        subtitle,
    })
}

fn free_tier_bar(label: &str, remaining: Option<u64>, total: Option<u64>) -> Option<LimitBar> {
    let (remaining, total) = (remaining?, total?);
    if total == 0 {
        return None;
    }

    let percent_left = ((remaining as f32 / total as f32) * 100.0).clamp(0.0, 100.0);
    let percent_used = (100.0 - percent_left).clamp(0.0, 100.0);

    Some(LimitBar {
        label: label.to_string(),
        percent_used,
        percent_left,
        reset_at: None,
        subtitle: Some(format!("{remaining} of {total} left")),
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct CopilotUsageResponse {
    #[serde(default)]
    copilot_plan: String,
    #[serde(default)]
    quota_reset_date: Option<String>,
    #[serde(default)]
    limited_user_reset_date: Option<String>,
    #[serde(default)]
    quota_snapshots: Option<QuotaSnapshots>,
    #[serde(default)]
    limited_user_quotas: Option<SimpleQuotas>,
    #[serde(default)]
    monthly_quotas: Option<SimpleQuotas>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct QuotaSnapshots {
    #[serde(default)]
    premium_interactions: Option<QuotaSnapshot>,
    #[serde(default)]
    chat: Option<QuotaSnapshot>,
    #[serde(default)]
    completions: Option<QuotaSnapshot>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct QuotaSnapshot {
    #[serde(default)]
    percent_remaining: Option<f32>,
    #[serde(default)]
    entitlement: Option<u64>,
    #[serde(default)]
    remaining: Option<u64>,
    #[serde(default)]
    unlimited: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct SimpleQuotas {
    #[serde(default)]
    chat: Option<u64>,
    #[serde(default)]
    completions: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    expires_in: u64,
    interval: u64,
}

#[derive(Debug, Deserialize)]
struct AccessTokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct DeviceFlowErrorResponse {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}
