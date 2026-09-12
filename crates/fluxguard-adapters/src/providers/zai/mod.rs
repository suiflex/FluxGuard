use ::time::OffsetDateTime;
use async_trait::async_trait;
use fluxguard_core::{
    BudgetSnapshot, MetricDimension, SourceDescriptor, SourceKind, SourceQuality,
};
use fluxguard_runtime::{BudgetSource, ProbeReport, SourceError};
use serde::Deserialize;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::support::{self, WindowSpec};

const OBSERVED_VIA: &str = "zai_coding_plan";

/// Z.AI GLM Coding Plan quota adapter.
#[derive(Clone, Debug)]
pub struct ZaiAdapter {
    descriptor: SourceDescriptor,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZaiCodingPlanStats {
    #[serde(default)]
    pub five_hour_requests_limit: Option<f64>,
    #[serde(default)]
    pub five_hour_requests_remaining: Option<f64>,
    #[serde(default)]
    pub five_hour_resets_at_unix: Option<i64>,
    #[serde(default)]
    pub weekly_requests_limit: Option<f64>,
    #[serde(default)]
    pub weekly_requests_remaining: Option<f64>,
    #[serde(default)]
    pub weekly_resets_at_unix: Option<i64>,
}

fn descriptor() -> SourceDescriptor {
    support::descriptor(
        "provider.zai",
        SourceKind::Provider,
        "Z.AI GLM Coding Plan",
        SourceQuality::OfficialStructured,
        support::capabilities(true, false),
    )
}

impl ZaiAdapter {
    pub fn new() -> Self {
        Self {
            descriptor: descriptor(),
        }
    }

    pub fn normalize(stats: ZaiCodingPlanStats) -> Result<BudgetSnapshot, SourceError> {
        let source = descriptor();
        let now = OffsetDateTime::now_utc();
        let mut windows = Vec::new();

        let quotas = [
            (
                "provider.zai.five_hour",
                "5-Hour Quota",
                5 * 3600,
                stats.five_hour_requests_remaining,
                stats.five_hour_requests_limit,
                stats.five_hour_resets_at_unix,
            ),
            (
                "provider.zai.weekly",
                "Weekly Quota",
                7 * 24 * 3600,
                stats.weekly_requests_remaining,
                stats.weekly_requests_limit,
                stats.weekly_resets_at_unix,
            ),
        ];
        for (id, label, duration, remaining, limit, reset_unix) in quotas {
            if remaining.is_none() || limit.is_none() {
                continue;
            }
            support::push_window(
                &mut windows,
                &source,
                OBSERVED_VIA,
                now,
                WindowSpec {
                    remaining,
                    limit,
                    window_duration_seconds: Some(duration),
                    resets_at: reset_unix
                        .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok()),
                    ..WindowSpec::new(id, label, MetricDimension::Requests)
                },
            )?;
        }

        Ok(support::snapshot(
            source,
            now,
            windows,
            "zai_stats_missing",
            "No Z.AI coding plan quota information available",
        ))
    }
}

impl Default for ZaiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BudgetSource for ZaiAdapter {
    fn descriptor(&self) -> SourceDescriptor {
        self.descriptor.clone()
    }

    async fn probe(&self) -> Result<ProbeReport, SourceError> {
        Ok(support::probe_env_keys(&["ZAI_API_KEY", "GLM_API_KEY"]))
    }

    // ponytail: no verified machine-readable quota surface yet, so report
    // unsupported instead of publishing an empty "allowed" snapshot. Wire a
    // real fetch here once an official surface exists.
    async fn refresh(&self) -> Result<BudgetSnapshot, SourceError> {
        Err(SourceError::UnsupportedVersion)
    }

    async fn run(
        &self,
        _updates: watch::Sender<fluxguard_runtime::SourceState>,
        _cancel: CancellationToken,
    ) -> Result<(), SourceError> {
        self.refresh().await.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zai_stats_normalizes_five_hour_and_weekly_coding_plan() {
        // Shape is FluxGuard's own normalized contract, not a vendor payload.
        let stats: ZaiCodingPlanStats = serde_json::from_str(include_str!(
            "../../../tests/fixtures/zai/coding_plan_basic.json"
        ))
        .expect("fixture");

        let snapshot = ZaiAdapter::normalize(stats).expect("normalize");
        assert_eq!(snapshot.windows.len(), 2);

        let five_h = &snapshot.windows[0];
        assert_eq!(five_h.dimension, MetricDimension::Requests);
        assert_eq!(five_h.remaining_percent, Some(85.0));
        assert!(five_h.resets_at.is_some());

        let weekly = &snapshot.windows[1];
        assert_eq!(weekly.dimension, MetricDimension::Requests);
        assert_eq!(weekly.remaining_percent, Some(92.0));
    }

    #[tokio::test]
    async fn detection_only_contract() {
        let empty = ZaiAdapter::normalize(ZaiCodingPlanStats::default()).expect("normalize");
        support::assert_detection_only(&ZaiAdapter::new(), empty).await;
    }
}
