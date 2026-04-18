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
pub struct ProviderSnapshot {
    pub kind: ProviderKind,
    pub visible: bool,
    pub confidence: Confidence,
    pub fetched_at: SystemTime,
    pub stale: bool,
    pub unavailable: bool,
    pub summary_bar: Option<LimitBar>,
    pub detail_bars: Vec<LimitBar>,
    pub notes: Vec<String>,
}
