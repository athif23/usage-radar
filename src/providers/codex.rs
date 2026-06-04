use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(target_os = "windows")]
use cookie_scoop::{get_cookies, BrowserName, CookieMode, GetCookiesOptions};
use regex::Regex;
use serde::{Deserialize, Deserializer};

use crate::providers::snapshot::{Confidence, CreditBalance, LimitBar};
use crate::providers::{ProviderKind, ProviderSnapshot};
use crate::storage::config as config_store;
use crate::util::paths;

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const CHATGPT_URL: &str = "https://chatgpt.com";
const ADMIN_BILLING_URL: &str = "https://chatgpt.com/admin/billing";
const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";
const ENV_CHATGPT_COOKIE_HEADER: &str = "USAGE_RADAR_CHATGPT_COOKIE_HEADER";

pub async fn fetch_snapshot() -> Result<ProviderSnapshot, String> {
    let auth = load_auth()?;
    let account_id = auth.tokens.account_id.clone();
    let token = auth
        .tokens
        .access_token
        .ok_or_else(|| "Codex auth.json does not contain an access token".to_string())?;

    let mut request = reqwest::Client::new()
        .get(USAGE_URL)
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, "usage-radar");

    if let Some(account_id) = account_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        request = request.header("ChatGPT-Account-Id", account_id);
    }

    let response = request
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

    if let Some(window) = usage
        .rate_limit
        .as_ref()
        .and_then(|rate_limit| rate_limit.primary_window.clone())
    {
        detail_bars.push(window.into_bar("5h window"));
    }

    if let Some(window) = usage
        .rate_limit
        .as_ref()
        .and_then(|rate_limit| rate_limit.secondary_window.clone())
    {
        detail_bars.push(window.into_bar("Weekly window"));
    }

    let summary_bar = detail_bars
        .iter()
        .cloned()
        .min_by(|left, right| left.percent_left.total_cmp(&right.percent_left));

    let mut notes = Vec::new();
    if let Some(plan_type) = usage.plan_type.as_deref() {
        notes.push(format!("Plan: {plan_type}"));
    }

    let credits = usage.credits.and_then(|credits| {
        if credits.has_credits || credits.unlimited || credits.balance.is_some() {
            Some(CreditBalance {
                remaining: credits.balance,
                unlimited: credits.unlimited,
                scope: Some(if account_id.is_some() {
                    "Codex workspace/account".to_string()
                } else {
                    "Codex account".to_string()
                }),
                captured_at: Some(SystemTime::now()),
            })
        } else {
            None
        }
    });

    let webview_refresh_note = refresh_webview_billing_credits().await;
    let mut web_credits = None;

    if let Some(webview_credits) = load_webview_billing_credits() {
        web_credits = Some(CreditBalance {
            remaining: Some(webview_credits.remaining),
            unlimited: false,
            scope: Some("OpenAI web billing".to_string()),
            captured_at: Some(webview_credits.captured_at()),
        });
        if let Some(note) = webview_refresh_note {
            notes.push(note);
        } else {
            notes.push(format!(
                "Billing credits captured by {}.",
                webview_credits.source
            ));
        }
    } else if credits
        .as_ref()
        .map(|credits| credits.remaining.is_none() && !credits.unlimited)
        .unwrap_or(true)
    {
        match fetch_admin_billing_credits().await {
            Ok(Some(admin_credits)) => {
                web_credits = Some(CreditBalance {
                    remaining: Some(admin_credits.remaining),
                    unlimited: false,
                    scope: Some("OpenAI web billing".to_string()),
                    captured_at: Some(SystemTime::now()),
                });
                notes.push(format!(
                    "Imported ChatGPT billing session from {}.",
                    admin_credits.source_label
                ));
            }
            Ok(None) => {
                notes.push("Billing credits fallback: no browser session found.".to_string());
            }
            Err(error) => {
                notes.push(format!("Billing credits fallback: {error}"));
            }
        }
    }

    if detail_bars.is_empty() && credits.is_none() && web_credits.is_none() {
        return Err("Codex usage response did not include limit windows or credits".to_string());
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
        credits,
        web_credits,
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
    #[serde(alias = "accessToken")]
    access_token: Option<String>,
    #[serde(alias = "accountId")]
    account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WhamUsage {
    plan_type: Option<String>,
    rate_limit: Option<RateLimit>,
    credits: Option<Credits>,
}

#[derive(Debug, Deserialize)]
struct RateLimit {
    primary_window: Option<UsageWindow>,
    secondary_window: Option<UsageWindow>,
}

#[derive(Debug, Clone, Deserialize)]
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
    #[serde(default, deserialize_with = "deserialize_optional_f64")]
    balance: Option<f64>,
    #[serde(default)]
    has_credits: bool,
    #[serde(default)]
    unlimited: bool,
}

#[derive(Debug)]
struct AdminBillingCredits {
    remaining: f64,
    source_label: String,
}

#[derive(Debug, Deserialize)]
struct WebViewBillingCredits {
    remaining: f64,
    source: String,
    captured_at_epoch_seconds: u64,
}

fn load_webview_billing_credits() -> Option<WebViewBillingCredits> {
    let path = paths::chatgpt_billing_file_path().ok()?;
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

async fn refresh_webview_billing_credits() -> Option<String> {
    if !paths::chatgpt_billing_webview_dir().ok()?.exists() {
        return None;
    }

    let before = load_webview_billing_credits().map(|credits| credits.captured_at_epoch_seconds);
    let path = chatgpt_billing_probe_path().ok()?;
    let status = Command::new(path)
        .arg("--background")
        .arg("--timeout-seconds")
        .arg("20")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match status {
        Ok(status) if status.success() => {
            let after =
                load_webview_billing_credits().map(|credits| credits.captured_at_epoch_seconds);
            if after.is_some() && after != before {
                Some("Billing credits refreshed from ChatGPT billing WebView.".to_string())
            } else {
                Some("Billing credits WebView refresh did not find a newer balance.".to_string())
            }
        }
        Ok(status) => Some(format!(
            "Billing credits WebView refresh exited with {status}."
        )),
        Err(error) => Some(format!(
            "Billing credits WebView refresh could not start: {error}"
        )),
    }
}

fn chatgpt_billing_probe_path() -> Result<PathBuf, String> {
    let current = std::env::current_exe()
        .map_err(|error| format!("Could not locate Usage Radar executable: {error}"))?;
    let directory = current
        .parent()
        .ok_or_else(|| format!("Executable has no parent directory: {}", current.display()))?;
    let executable = if cfg!(target_os = "windows") {
        "chatgpt_billing_probe.exe"
    } else {
        "chatgpt_billing_probe"
    };
    let path = directory.join(executable);

    if path.exists() {
        Ok(path)
    } else {
        Err(format!(
            "ChatGPT billing balance helper was not found at {}.",
            path.display()
        ))
    }
}

impl WebViewBillingCredits {
    fn captured_at(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(self.captured_at_epoch_seconds)
    }
}

async fn fetch_admin_billing_credits() -> Result<Option<AdminBillingCredits>, String> {
    let Some(cookie_header) = resolve_chatgpt_cookie_header().await else {
        return Ok(None);
    };
    let response = reqwest::Client::new()
        .get(ADMIN_BILLING_URL)
        .header(reqwest::header::COOKIE, &cookie_header.header)
        .header(reqwest::header::USER_AGENT, BROWSER_USER_AGENT)
        .header(
            reqwest::header::ACCEPT,
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|error| format!("could not reach ChatGPT billing page: {error}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("ChatGPT billing page returned {status}"));
    }

    let text = response
        .text()
        .await
        .map_err(|error| format!("could not read ChatGPT billing page: {error}"))?;

    if looks_like_chatgpt_signed_out(&text) {
        return Err(format!(
            "browser session from {} is signed out",
            cookie_header.source_label
        ));
    }

    let Some(remaining) = parse_admin_billing_credits(&text) else {
        return Err(format!(
            "billing page from {} did not include a credit balance",
            cookie_header.source_label
        ));
    };

    Ok(Some(AdminBillingCredits {
        remaining,
        source_label: cookie_header.source_label,
    }))
}

fn looks_like_chatgpt_signed_out(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("auth/login")
        || lower.contains("log in")
        || lower.contains("sign in")
        || lower.contains("login required")
}

fn parse_admin_billing_credits(text: &str) -> Option<f64> {
    let text = strip_html_tags(text);
    let patterns = [
        r#"(?is)(?:saldo\s+kredit|credit\s+balance|credits?\s+balance)[^0-9]{0,120}([0-9][0-9.,\s]*)"#,
        r#"(?is)(?:remaining\s+credits?|credits?\s+remaining)[^0-9]{0,120}([0-9][0-9.,\s]*)"#,
    ];

    patterns.iter().find_map(|pattern| {
        Regex::new(pattern)
            .ok()?
            .captures(&text)
            .and_then(|captures| captures.get(1))
            .and_then(|capture| parse_credit_number(capture.as_str()))
    })
}

fn strip_html_tags(text: &str) -> String {
    Regex::new(r#"(?is)<[^>]+>"#)
        .map(|regex| regex.replace_all(text, " ").into_owned())
        .unwrap_or_else(|_| text.to_string())
}

fn parse_credit_number(raw: &str) -> Option<f64> {
    let token = raw
        .trim()
        .replace([' ', '\u{00A0}', '\u{202F}'], "")
        .trim_matches(|character: char| {
            !character.is_ascii_digit() && character != '.' && character != ','
        })
        .to_string();

    if token.is_empty() {
        return None;
    }

    let normalized = if token.contains('.') && token.contains(',') {
        let last_dot = token.rfind('.').unwrap_or(0);
        let last_comma = token.rfind(',').unwrap_or(0);
        if last_dot > last_comma {
            token.replace(',', "")
        } else {
            token.replace('.', "").replace(',', ".")
        }
    } else if token.contains('.') {
        if looks_like_grouped_number(&token, '.') {
            token.replace('.', "")
        } else {
            token
        }
    } else if token.contains(',') {
        if looks_like_grouped_number(&token, ',') {
            token.replace(',', "")
        } else {
            token.replace(',', ".")
        }
    } else {
        token
    };

    normalized.parse::<f64>().ok()
}

fn looks_like_grouped_number(token: &str, separator: char) -> bool {
    let parts = token.split(separator).collect::<Vec<_>>();
    parts.len() > 1
        && parts[0].chars().all(|character| character.is_ascii_digit())
        && (1..=3).contains(&parts[0].len())
        && parts[1..]
            .iter()
            .all(|part| part.len() == 3 && part.chars().all(|character| character.is_ascii_digit()))
}

#[derive(Debug)]
struct ChatGptCookieHeader {
    header: String,
    source_label: String,
}

async fn resolve_chatgpt_cookie_header() -> Option<ChatGptCookieHeader> {
    if let Some(header) = configured_chatgpt_cookie_header() {
        return Some(ChatGptCookieHeader {
            header,
            source_label: "configured Cookie header".to_string(),
        });
    }

    import_chatgpt_cookie_header().await
}

fn configured_chatgpt_cookie_header() -> Option<String> {
    if let Some(header) = std::env::var(ENV_CHATGPT_COOKIE_HEADER)
        .ok()
        .and_then(|value| normalize_cookie_header(&value))
    {
        return Some(header);
    }

    let path = paths::config_file_path().ok()?;
    let config = config_store::load(&path).ok().flatten()?;
    normalize_cookie_header(config.codex_chatgpt_cookie_header.as_deref()?)
}

fn normalize_cookie_header(raw: &str) -> Option<String> {
    let parts = raw
        .split(';')
        .filter_map(|part| {
            let trimmed = part.trim();
            let (name, value) = trimmed.split_once('=')?;
            let name = name.trim();
            let value = value.trim();
            if name.is_empty() || value.is_empty() {
                None
            } else {
                Some(format!("{name}={value}"))
            }
        })
        .collect::<Vec<_>>();

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy)]
enum WindowsBrowser {
    Chrome,
    Brave,
    Edge,
}

#[cfg(target_os = "windows")]
impl WindowsBrowser {
    fn label(self) -> &'static str {
        match self {
            Self::Chrome => "Chrome",
            Self::Brave => "Brave",
            Self::Edge => "Edge",
        }
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
struct BrowserProfileSource {
    browser: WindowsBrowser,
    profile_path: String,
    source_label: String,
}

async fn import_chatgpt_cookie_header() -> Option<ChatGptCookieHeader> {
    #[cfg(target_os = "windows")]
    {
        for source in browser_profile_sources() {
            if let Some(header) = chatgpt_cookie_header_for_source(&source).await {
                return Some(ChatGptCookieHeader {
                    header,
                    source_label: source.source_label,
                });
            }
        }
    }

    None
}

#[cfg(target_os = "windows")]
async fn chatgpt_cookie_header_for_source(source: &BrowserProfileSource) -> Option<String> {
    let origins = vec![
        CHATGPT_URL.to_string(),
        "https://auth.openai.com".to_string(),
        "https://openai.com".to_string(),
    ];
    let names = vec![
        "__Secure-next-auth.session-token".to_string(),
        "__Secure-next-auth.session-token.0".to_string(),
        "__Secure-next-auth.session-token.1".to_string(),
        "__Host-next-auth.csrf-token".to_string(),
        "__Secure-next-auth.callback-url".to_string(),
        "__cf_bm".to_string(),
        "_cfuvid".to_string(),
        "cf_clearance".to_string(),
        "oai-did".to_string(),
        "oai-sc".to_string(),
    ];

    let options = match source.browser {
        WindowsBrowser::Chrome | WindowsBrowser::Brave => GetCookiesOptions::new(CHATGPT_URL)
            .origins(origins)
            .names(names)
            .browsers(vec![BrowserName::Chrome])
            .chrome_profile(source.profile_path.clone())
            .mode(CookieMode::First),
        WindowsBrowser::Edge => GetCookiesOptions::new(CHATGPT_URL)
            .origins(origins)
            .names(names)
            .browsers(vec![BrowserName::Edge])
            .edge_profile(source.profile_path.clone())
            .mode(CookieMode::First),
    };

    let result = get_cookies(options).await;
    cookie_header(&result.cookies)
}

#[cfg(target_os = "windows")]
fn cookie_header(cookies: &[cookie_scoop::Cookie]) -> Option<String> {
    let mut parts = Vec::new();

    for cookie in cookies {
        let name = cookie.name.trim();
        let value = cookie.value.trim();
        if name.is_empty() || value.is_empty() {
            continue;
        }

        parts.push(format!("{name}={value}"));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}

#[cfg(target_os = "windows")]
fn browser_profile_sources() -> Vec<BrowserProfileSource> {
    let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") else {
        return Vec::new();
    };

    let local_app_data = PathBuf::from(local_app_data);
    let mut sources = Vec::new();

    sources.extend(chromium_profile_sources(
        WindowsBrowser::Chrome,
        local_app_data.join("Google/Chrome/User Data"),
    ));
    sources.extend(chromium_profile_sources(
        WindowsBrowser::Brave,
        local_app_data.join("BraveSoftware/Brave-Browser/User Data"),
    ));
    sources.extend(chromium_profile_sources(
        WindowsBrowser::Edge,
        local_app_data.join("Microsoft/Edge/User Data"),
    ));

    sources
}

#[cfg(target_os = "windows")]
fn chromium_profile_sources(
    browser: WindowsBrowser,
    user_data_dir: PathBuf,
) -> Vec<BrowserProfileSource> {
    chromium_profile_dirs(&user_data_dir)
        .into_iter()
        .map(|profile_dir| {
            let profile_name = profile_dir
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Profile")
                .to_string();

            BrowserProfileSource {
                browser,
                profile_path: profile_name.clone(),
                source_label: format!("{} ({profile_name})", browser.label()),
            }
        })
        .collect()
}

#[cfg(target_os = "windows")]
fn chromium_profile_dirs(user_data_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(user_data_dir) else {
        return Vec::new();
    };

    let mut defaults = Vec::new();
    let mut others = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let Some(profile_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        if !matches!(profile_name, "Default") && !profile_name.starts_with("Profile ") {
            continue;
        }

        if !has_cookie_db(&path) {
            continue;
        }

        if profile_name == "Default" {
            defaults.push(path);
        } else {
            others.push(path);
        }
    }

    others.sort_by(|left, right| {
        left.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .cmp(
                right
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default(),
            )
    });

    defaults.extend(others);
    defaults
}

#[cfg(target_os = "windows")]
fn has_cookie_db(profile_dir: &Path) -> bool {
    profile_dir.join("Network").join("Cookies").exists() || profile_dir.join("Cookies").exists()
}

fn deserialize_optional_f64<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;

    Ok(match value {
        Some(serde_json::Value::Number(number)) => number.as_f64(),
        Some(serde_json::Value::String(text)) => text.trim().parse::<f64>().ok(),
        _ => None,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_numeric_credit_balance() {
        let usage: WhamUsage = serde_json::from_str(
            r#"{
                "rate_limit": null,
                "credits": {
                    "has_credits": true,
                    "unlimited": false,
                    "balance": 42.5
                }
            }"#,
        )
        .expect("credit payload should decode");

        let credits = usage.credits.expect("credits should be present");
        assert_eq!(credits.balance, Some(42.5));
        assert!(credits.has_credits);
        assert!(!credits.unlimited);
    }

    #[test]
    fn decodes_string_credit_balance() {
        let usage: WhamUsage = serde_json::from_str(
            r#"{
                "rate_limit": null,
                "credits": {
                    "has_credits": true,
                    "balance": "112.4"
                }
            }"#,
        )
        .expect("credit payload should decode");

        let credits = usage.credits.expect("credits should be present");
        assert_eq!(credits.balance, Some(112.4));
    }

    #[test]
    fn decodes_tokens_with_workspace_account_id_alias() {
        let auth: CodexAuthFile = serde_json::from_str(
            r#"{
                "tokens": {
                    "accessToken": "access",
                    "accountId": "account-team"
                }
            }"#,
        )
        .expect("auth payload should decode");

        assert_eq!(auth.tokens.access_token.as_deref(), Some("access"));
        assert_eq!(auth.tokens.account_id.as_deref(), Some("account-team"));
    }

    #[test]
    fn parses_indonesian_admin_billing_credit_balance() {
        let html = r#"
            <section>
              <h2>Saldo Kredit</h2>
              <div>24.794 / Rp 16.364.274,09</div>
            </section>
        "#;

        assert_eq!(parse_admin_billing_credits(html), Some(24_794.0));
    }

    #[test]
    fn parses_english_admin_billing_credit_balance() {
        let html = r#"
            <section>
              <h2>Credit balance</h2>
              <div>24,794 / $1,000.00</div>
            </section>
        "#;

        assert_eq!(parse_admin_billing_credits(html), Some(24_794.0));
    }
}
