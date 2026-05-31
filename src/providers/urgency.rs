use std::cmp::Ordering;
use std::time::SystemTime;

use crate::providers::{ProviderKind, ProviderSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum UrgencyTier {
    Critical,
    Warning,
    Normal,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct UrgencyKey {
    tier: UrgencyTier,
    percent_left: f32,
    reset_at: Option<SystemTime>,
    provider_order: usize,
}

pub fn sort_by_usage_urgency(providers: &mut [ProviderKind], snapshots: &[ProviderSnapshot]) {
    providers.sort_by(|left, right| {
        urgency_key(*left, snapshots).cmp_with(urgency_key(*right, snapshots))
    });
}

fn urgency_key(kind: ProviderKind, snapshots: &[ProviderSnapshot]) -> UrgencyKey {
    let provider_order = provider_order(kind);
    let Some(snapshot) = snapshots.iter().find(|snapshot| snapshot.kind == kind) else {
        return UrgencyKey::unknown(provider_order);
    };

    if snapshot.unavailable {
        return UrgencyKey::unknown(provider_order);
    }

    let Some(bar) = snapshot.summary_bar.as_ref() else {
        return UrgencyKey::unknown(provider_order);
    };

    let percent_left = bar.percent_left.clamp(0.0, 100.0);

    UrgencyKey {
        tier: tier_for_percent_left(percent_left),
        percent_left,
        reset_at: bar.reset_at,
        provider_order,
    }
}

impl UrgencyKey {
    fn unknown(provider_order: usize) -> Self {
        Self {
            tier: UrgencyTier::Unknown,
            percent_left: 100.0,
            reset_at: None,
            provider_order,
        }
    }

    fn cmp_with(self, other: Self) -> Ordering {
        self.tier
            .cmp(&other.tier)
            .then_with(|| self.percent_left.total_cmp(&other.percent_left))
            .then_with(|| compare_reset_at(self.reset_at, other.reset_at))
            .then_with(|| self.provider_order.cmp(&other.provider_order))
    }
}

fn tier_for_percent_left(percent_left: f32) -> UrgencyTier {
    if percent_left <= 5.0 {
        UrgencyTier::Critical
    } else if percent_left <= 15.0 {
        UrgencyTier::Warning
    } else {
        UrgencyTier::Normal
    }
}

fn compare_reset_at(left: Option<SystemTime>, right: Option<SystemTime>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn provider_order(kind: ProviderKind) -> usize {
    match kind {
        ProviderKind::Codex => 0,
        ProviderKind::Copilot => 1,
        ProviderKind::OpenCodeGo => 2,
        ProviderKind::ClaudeCode => 3,
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use crate::providers::{Confidence, LimitBar};

    use super::*;

    #[test]
    fn critical_providers_sort_before_warning_and_normal() {
        let snapshots = vec![
            snapshot(ProviderKind::Codex, 40.0, Some(900), false),
            snapshot(ProviderKind::Copilot, 12.0, Some(600), false),
            snapshot(ProviderKind::OpenCodeGo, 4.0, Some(300), false),
        ];
        let mut providers = vec![
            ProviderKind::Codex,
            ProviderKind::Copilot,
            ProviderKind::OpenCodeGo,
        ];

        sort_by_usage_urgency(&mut providers, &snapshots);

        assert_eq!(
            providers,
            vec![
                ProviderKind::OpenCodeGo,
                ProviderKind::Copilot,
                ProviderKind::Codex,
            ]
        );
    }

    #[test]
    fn lower_percent_left_wins_within_the_same_tier() {
        let snapshots = vec![
            snapshot(ProviderKind::Codex, 10.0, Some(900), false),
            snapshot(ProviderKind::Copilot, 7.0, Some(600), false),
        ];
        let mut providers = vec![ProviderKind::Codex, ProviderKind::Copilot];

        sort_by_usage_urgency(&mut providers, &snapshots);

        assert_eq!(providers, vec![ProviderKind::Copilot, ProviderKind::Codex]);
    }

    #[test]
    fn earlier_reset_wins_when_urgency_is_tied() {
        let snapshots = vec![
            snapshot(ProviderKind::Codex, 20.0, Some(900), false),
            snapshot(ProviderKind::Copilot, 20.0, Some(300), false),
        ];
        let mut providers = vec![ProviderKind::Codex, ProviderKind::Copilot];

        sort_by_usage_urgency(&mut providers, &snapshots);

        assert_eq!(providers, vec![ProviderKind::Copilot, ProviderKind::Codex]);
    }

    #[test]
    fn unknown_or_unavailable_providers_sort_after_usable_snapshots() {
        let snapshots = vec![
            snapshot(ProviderKind::Codex, 90.0, Some(900), false),
            snapshot(ProviderKind::Copilot, 2.0, Some(300), true),
        ];
        let mut providers = vec![
            ProviderKind::Copilot,
            ProviderKind::OpenCodeGo,
            ProviderKind::Codex,
        ];

        sort_by_usage_urgency(&mut providers, &snapshots);

        assert_eq!(
            providers,
            vec![
                ProviderKind::Codex,
                ProviderKind::Copilot,
                ProviderKind::OpenCodeGo,
            ]
        );
    }

    fn snapshot(
        kind: ProviderKind,
        percent_left: f32,
        reset_after_seconds: Option<u64>,
        unavailable: bool,
    ) -> ProviderSnapshot {
        let reset_at = reset_after_seconds.map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds));
        let bar = LimitBar {
            label: "Test window".to_string(),
            percent_used: 100.0 - percent_left,
            percent_left,
            reset_at,
            subtitle: None,
        };

        ProviderSnapshot {
            kind,
            visible: true,
            confidence: Confidence::Exact,
            fetched_at: UNIX_EPOCH,
            stale: false,
            unavailable,
            summary_bar: Some(bar.clone()),
            detail_bars: vec![bar],
            notes: Vec::new(),
        }
    }
}
