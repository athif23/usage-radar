use std::time::{Duration, SystemTime, UNIX_EPOCH};

use regex::Regex;

use crate::providers::snapshot::{Confidence, LimitBar};
use crate::providers::{ProviderKind, ProviderSnapshot};
use crate::storage::config as config_store;
use crate::util::paths;

const BASE_URL: &str = "https://opencode.ai";
const SERVER_URL: &str = "https://opencode.ai/_server";
const WORKSPACES_SERVER_ID: &str =
    "def39973159c7f0483d8793a822b8dbb10d067e12c65455fcb4608459ba0234f";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";
const ENV_COOKIE_HEADER: &str = "OPENCODE_GO_COOKIE_HEADER";
const ENV_WORKSPACE_ID: &str = "OPENCODE_GO_WORKSPACE_ID";

pub async fn fetch_snapshot() -> Result<ProviderSnapshot, String> {
    let settings = load_settings()?;
    let Some(cookie_header) = request_cookie_header(settings.cookie_header.as_deref()) else {
        return Ok(disconnected_snapshot(
            "Add an OpenCode Go Cookie header to OPENCODE_GO_COOKIE_HEADER or config.json to connect this provider.",
        ));
    };

    let workspace_id =
        if let Some(workspace_id) = normalize_workspace_id(settings.workspace_id.as_deref()) {
            workspace_id
        } else {
            match fetch_workspace_id(&cookie_header).await {
                Ok(workspace_id) => workspace_id,
                Err(OpenCodeGoFetchError::InvalidCredentials) => {
                    return Ok(disconnected_snapshot(
                    "OpenCode Go rejected the saved cookie. Update it to reconnect this provider.",
                ));
                }
                Err(OpenCodeGoFetchError::Message(error)) => return Err(error),
            }
        };

    let usage_page = match fetch_usage_page(&workspace_id, &cookie_header).await {
        Ok(usage_page) => usage_page,
        Err(OpenCodeGoFetchError::InvalidCredentials) => {
            return Ok(disconnected_snapshot(
                "OpenCode Go rejected the saved cookie. Update it to reconnect this provider.",
            ));
        }
        Err(OpenCodeGoFetchError::Message(error)) => return Err(error),
    };

    let usage = parse_usage_page(&usage_page)?;
    let fetched_at = SystemTime::now();

    let mut detail_bars = vec![
        usage_bar(
            "5h window",
            usage.rolling_usage_percent,
            usage.rolling_reset_in_sec,
            fetched_at,
        ),
        usage_bar(
            "Weekly window",
            usage.weekly_usage_percent,
            usage.weekly_reset_in_sec,
            fetched_at,
        ),
    ];

    if let Some((percent, reset_in_sec)) = usage.monthly_usage {
        detail_bars.push(usage_bar(
            "Monthly window",
            percent,
            reset_in_sec,
            fetched_at,
        ));
    }

    let summary_bar = detail_bars
        .iter()
        .cloned()
        .min_by(|left, right| left.percent_left.total_cmp(&right.percent_left));

    Ok(ProviderSnapshot {
        kind: ProviderKind::OpenCodeGo,
        visible: true,
        confidence: Confidence::Exact,
        fetched_at,
        stale: false,
        unavailable: false,
        summary_bar,
        detail_bars,
        notes: Vec::new(),
    })
}

#[derive(Debug)]
struct OpenCodeGoSettings {
    cookie_header: Option<String>,
    workspace_id: Option<String>,
}

#[derive(Debug)]
struct OpenCodeGoUsage {
    rolling_usage_percent: f32,
    rolling_reset_in_sec: u64,
    weekly_usage_percent: f32,
    weekly_reset_in_sec: u64,
    monthly_usage: Option<(f32, u64)>,
}

enum OpenCodeGoFetchError {
    InvalidCredentials,
    Message(String),
}

fn load_settings() -> Result<OpenCodeGoSettings, String> {
    let env_cookie_header = std::env::var(ENV_COOKIE_HEADER)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let env_workspace_id = std::env::var(ENV_WORKSPACE_ID)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let file_settings = match paths::config_file_path() {
        Ok(path) => match config_store::load(&path) {
            Ok(Some(config)) => OpenCodeGoSettings {
                cookie_header: config.opencode_go_cookie_header,
                workspace_id: config.opencode_go_workspace_id,
            },
            Ok(None) => OpenCodeGoSettings {
                cookie_header: None,
                workspace_id: None,
            },
            Err(error) => return Err(error),
        },
        Err(_) => OpenCodeGoSettings {
            cookie_header: None,
            workspace_id: None,
        },
    };

    Ok(OpenCodeGoSettings {
        cookie_header: env_cookie_header.or(file_settings.cookie_header),
        workspace_id: env_workspace_id.or(file_settings.workspace_id),
    })
}

fn request_cookie_header(raw_header: Option<&str>) -> Option<String> {
    let raw_header = raw_header?.trim();
    if raw_header.is_empty() {
        return None;
    }

    let mut cookies = Vec::new();

    for part in raw_header.split(';') {
        let trimmed = part.trim();
        let Some((name, value)) = trimmed.split_once('=') else {
            continue;
        };
        let name = name.trim();
        let value = value.trim();

        if value.is_empty() {
            continue;
        }

        if matches!(name, "auth" | "__Host-auth") {
            cookies.push(format!("{name}={value}"));
        }
    }

    if cookies.is_empty() {
        None
    } else {
        Some(cookies.join("; "))
    }
}

fn normalize_workspace_id(raw: Option<&str>) -> Option<String> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }

    if raw.starts_with("wrk_") && raw.len() > 4 {
        return Some(raw.to_string());
    }

    if let Some(found) = Regex::new(r#"wrk_[A-Za-z0-9]+"#)
        .ok()?
        .find(raw)
        .map(|capture| capture.as_str().to_string())
    {
        return Some(found);
    }

    None
}

async fn fetch_workspace_id(cookie_header: &str) -> Result<String, OpenCodeGoFetchError> {
    let get_url = format!("{SERVER_URL}?id={WORKSPACES_SERVER_ID}");

    match fetch_text(
        &get_url,
        "GET",
        None,
        cookie_header,
        BASE_URL,
        Some(WORKSPACES_SERVER_ID),
    )
    .await
    {
        Ok(text) => parse_workspace_id(&text).ok_or_else(|| {
            OpenCodeGoFetchError::Message(
                "OpenCode Go workspace lookup did not return a workspace id.".to_string(),
            )
        }),
        Err(OpenCodeGoFetchError::Message(_)) => {
            let text = fetch_text(
                SERVER_URL,
                "POST",
                Some("[]"),
                cookie_header,
                BASE_URL,
                Some(WORKSPACES_SERVER_ID),
            )
            .await?;

            parse_workspace_id(&text).ok_or_else(|| {
                OpenCodeGoFetchError::Message(
                    "OpenCode Go workspace lookup did not return a workspace id.".to_string(),
                )
            })
        }
        Err(error) => Err(error),
    }
}

fn parse_workspace_id(text: &str) -> Option<String> {
    let regex = Regex::new(r#"id\s*:\s*"(wrk_[^"]+)""#).ok()?;
    regex
        .captures(text)
        .and_then(|captures| captures.get(1))
        .map(|capture| capture.as_str().to_string())
        .or_else(|| normalize_workspace_id(Some(text)))
}

async fn fetch_usage_page(
    workspace_id: &str,
    cookie_header: &str,
) -> Result<String, OpenCodeGoFetchError> {
    let url = format!("{BASE_URL}/workspace/{workspace_id}/go");
    let text = fetch_text(&url, "GET", None, cookie_header, &url, None).await?;

    if !looks_like_usage_page(&text) {
        return Err(OpenCodeGoFetchError::Message(
            "OpenCode Go usage page did not include usage fields.".to_string(),
        ));
    }

    Ok(text)
}

async fn fetch_text(
    url: &str,
    method: &str,
    body: Option<&str>,
    cookie_header: &str,
    referer: &str,
    server_id: Option<&str>,
) -> Result<String, OpenCodeGoFetchError> {
    let client = reqwest::Client::new();
    let accept = if server_id.is_some() || method != "GET" {
        "text/javascript, application/json;q=0.9, */*;q=0.8"
    } else {
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
    };

    let mut request = client
        .request(
            reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET),
            url,
        )
        .header(reqwest::header::COOKIE, cookie_header)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, accept)
        .header(reqwest::header::REFERER, referer)
        .timeout(Duration::from_secs(20));

    if let Some(server_id) = server_id {
        let instance_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        request = request
            .header("X-Server-Id", server_id)
            .header(
                "X-Server-Instance",
                format!("server-fn:usage-radar-{instance_id}"),
            )
            .header(reqwest::header::ORIGIN, BASE_URL);
    }

    if let Some(body) = body {
        request = request
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string());
    }

    let response = request.send().await.map_err(|error| {
        OpenCodeGoFetchError::Message(format!("Could not reach OpenCode Go: {error}"))
    })?;

    let status = response.status();
    let text = response.text().await.map_err(|error| {
        OpenCodeGoFetchError::Message(format!("Could not read OpenCode Go response: {error}"))
    })?;

    if status == reqwest::StatusCode::UNAUTHORIZED
        || status == reqwest::StatusCode::FORBIDDEN
        || looks_signed_out(&text)
    {
        return Err(OpenCodeGoFetchError::InvalidCredentials);
    }

    if !status.is_success() {
        return Err(OpenCodeGoFetchError::Message(format!(
            "OpenCode Go returned {status}",
        )));
    }

    Ok(text)
}

fn looks_signed_out(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("sign in")
        || lower.contains("login")
        || lower.contains("auth/authorize")
        || lower.contains("not associated with an account")
        || lower.contains("actor of type \"public\"")
}

fn looks_like_usage_page(text: &str) -> bool {
    text.contains("rollingUsage") && text.contains("weeklyUsage")
}

fn parse_usage_page(text: &str) -> Result<OpenCodeGoUsage, String> {
    let rolling_usage_percent = capture_float(
        r#"rollingUsage[^}]*?usagePercent\s*:\s*([0-9]+(?:\.[0-9]+)?)"#,
        text,
    )
    .ok_or_else(|| "OpenCode Go usage page is missing rolling usage percent.".to_string())?;
    let rolling_reset_in_sec = capture_u64(r#"rollingUsage[^}]*?resetInSec\s*:\s*([0-9]+)"#, text)
        .ok_or_else(|| "OpenCode Go usage page is missing rolling reset timing.".to_string())?;

    let weekly_usage_percent = capture_float(
        r#"weeklyUsage[^}]*?usagePercent\s*:\s*([0-9]+(?:\.[0-9]+)?)"#,
        text,
    )
    .ok_or_else(|| "OpenCode Go usage page is missing weekly usage percent.".to_string())?;
    let weekly_reset_in_sec = capture_u64(r#"weeklyUsage[^}]*?resetInSec\s*:\s*([0-9]+)"#, text)
        .ok_or_else(|| "OpenCode Go usage page is missing weekly reset timing.".to_string())?;

    let monthly_usage_percent = capture_float(
        r#"monthlyUsage[^}]*?usagePercent\s*:\s*([0-9]+(?:\.[0-9]+)?)"#,
        text,
    );
    let monthly_reset_in_sec = capture_u64(r#"monthlyUsage[^}]*?resetInSec\s*:\s*([0-9]+)"#, text);

    Ok(OpenCodeGoUsage {
        rolling_usage_percent,
        rolling_reset_in_sec,
        weekly_usage_percent,
        weekly_reset_in_sec,
        monthly_usage: monthly_usage_percent.zip(monthly_reset_in_sec),
    })
}

fn capture_float(pattern: &str, text: &str) -> Option<f32> {
    let regex = Regex::new(pattern).ok()?;
    let value = regex
        .captures(text)?
        .get(1)
        .map(|capture| capture.as_str())?;
    let value = value.parse::<f32>().ok()?;

    if (0.0..=1.0).contains(&value) {
        Some((value * 100.0).clamp(0.0, 100.0))
    } else {
        Some(value.clamp(0.0, 100.0))
    }
}

fn capture_u64(pattern: &str, text: &str) -> Option<u64> {
    let regex = Regex::new(pattern).ok()?;
    regex
        .captures(text)?
        .get(1)
        .and_then(|capture| capture.as_str().parse::<u64>().ok())
}

fn usage_bar(
    label: &str,
    percent_used: f32,
    reset_in_sec: u64,
    fetched_at: SystemTime,
) -> LimitBar {
    let percent_used = percent_used.clamp(0.0, 100.0);
    let percent_left = (100.0 - percent_used).clamp(0.0, 100.0);
    let reset_at = fetched_at.checked_add(Duration::from_secs(reset_in_sec));

    LimitBar {
        label: label.to_string(),
        percent_used,
        percent_left,
        reset_at,
        subtitle: Some(format!("Resets in {}", human_duration(reset_in_sec))),
    }
}

fn disconnected_snapshot(message: &str) -> ProviderSnapshot {
    ProviderSnapshot {
        kind: ProviderKind::OpenCodeGo,
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
