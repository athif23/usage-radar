use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::providers::ProviderKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Exact,
    Estimated,
    Partial,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitBar {
    pub label: String,
    pub percent_used: f32,
    pub percent_left: f32,
    pub reset_at: Option<SystemTime>,
    pub subtitle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditBalance {
    pub remaining: Option<f64>,
    pub unlimited: bool,
    pub scope: Option<String>,
    #[serde(default)]
    pub captured_at: Option<SystemTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetBank {
    pub available_count: u32,
    #[serde(default)]
    pub expires_at: Vec<SystemTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSnapshot {
    pub kind: ProviderKind,
    pub visible: bool,
    pub confidence: Confidence,
    pub fetched_at: SystemTime,
    pub stale: bool,
    pub unavailable: bool,
    pub summary_bar: Option<LimitBar>,
    pub detail_bars: Vec<LimitBar>,
    #[serde(default)]
    pub reset_bank: Option<ResetBank>,
    #[serde(default)]
    pub credits: Option<CreditBalance>,
    #[serde(default)]
    pub web_credits: Option<CreditBalance>,
    pub notes: Vec<String>,
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use super::*;

    #[test]
    fn decodes_cache_snapshot_without_reset_bank() {
        let snapshot = ProviderSnapshot {
            kind: ProviderKind::Codex,
            visible: true,
            confidence: Confidence::Exact,
            fetched_at: UNIX_EPOCH,
            stale: false,
            unavailable: false,
            summary_bar: None,
            detail_bars: Vec::new(),
            reset_bank: Some(ResetBank {
                available_count: 3,
                expires_at: vec![UNIX_EPOCH],
            }),
            credits: None,
            web_credits: None,
            notes: Vec::new(),
        };
        let mut value = serde_json::to_value(snapshot).expect("snapshot should encode");
        value
            .as_object_mut()
            .expect("snapshot should encode as an object")
            .remove("reset_bank");

        let decoded: ProviderSnapshot =
            serde_json::from_value(value).expect("older cache snapshot should decode");

        assert!(decoded.reset_bank.is_none());
    }
}
