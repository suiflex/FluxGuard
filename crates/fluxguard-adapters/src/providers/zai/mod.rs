use ::time::{Duration as SignedDuration, OffsetDateTime};
use async_trait::async_trait;
use fluxguard_core::{
    Applicability, Availability, BudgetSnapshot, BudgetWindow, DecimalValue, Freshness,
    MetricDimension, Provenance, SnapshotWarning, SourceCapabilities, SourceDescriptor, SourceId,
    SourceKind, SourceQuality, WindowId,
};
use fluxguard_runtime::{BudgetSource, ProbeReport, ProbeState, SourceError};
use serde::Deserialize;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

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

impl ZaiAdapter {
    pub fn new() -> Self {
        let id = SourceId::new("provider.zai").expect("static source id is valid");
        Self {
            descriptor: SourceDescriptor {
                id,
                kind: SourceKind::Provider,
                display_name: "Z.AI GLM Coding Plan".into(),
                adapter_version: env!("CARGO_PKG_VERSION").into(),
                source_quality: SourceQuality::OfficialStructured,
                capabilities: SourceCapabilities {
                    supports_snapshot: true,
                    supports_push_updates: false,
                    supports_reset_time: true,
                    supports_exact_remaining_percent: true,
                    supports_model_scope: true,
                    supports_cost: false,
                },
            },
        }
    }

    pub fn normalize(stats: ZaiCodingPlanStats) -> Result<BudgetSnapshot, SourceError> {
        let source_id = SourceId::new("provider.zai").map_err(|_| SourceError::InvalidPayload)?;
        let descriptor = SourceDescriptor {
            id: source_id.clone(),
            kind: SourceKind::Provider,
            display_name: "Z.AI GLM Coding Plan".into(),
            adapter_version: env!("CARGO_PKG_VERSION").into(),
            source_quality: SourceQuality::OfficialStructured,
            capabilities: SourceCapabilities {
                supports_snapshot: true,
                supports_push_updates: false,
                supports_reset_time: true,
                supports_exact_remaining_percent: true,
                supports_model_scope: true,
                supports_cost: false,
            },
        };

        let now = OffsetDateTime::now_utc();
        let mut windows = Vec::new();
        let mut warnings = Vec::new();

        if let (Some(rem), Some(limit)) = (
            stats.five_hour_requests_remaining,
            stats.five_hour_requests_limit,
        ) {
            if let (Ok(rem_dec), Ok(limit_dec)) =
                (DecimalValue::try_new(rem), DecimalValue::try_new(limit))
            {
                let remaining_percent = if limit > 0.0 {
                    Some((rem / limit) * 100.0)
                } else {
                    None
                };

                let resets_at = stats
                    .five_hour_resets_at_unix
                    .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok());

                let window_id = WindowId::new("provider.zai.five_hour")
                    .map_err(|_| SourceError::InvalidPayload)?;

                windows.push(BudgetWindow {
                    id: window_id,
                    label: Some("5-Hour Quota".into()),
                    dimension: MetricDimension::Requests,
                    used: DecimalValue::try_new((limit - rem).max(0.0)).ok(),
                    limit: Some(limit_dec),
                    remaining: Some(rem_dec),
                    used_percent: remaining_percent.map(|r| (100.0 - r).clamp(0.0, 100.0)),
                    remaining_percent,
                    window_duration_seconds: Some(5 * 3600),
                    resets_at,
                    fresh_until: Some(now + SignedDuration::seconds(60)),
                    hard_blocked: rem <= 0.0,
                    applicability: Applicability::Applicable,
                    observed_at: now,
                    freshness: Freshness::Fresh,
                    provenance: Provenance {
                        source_id: source_id.clone(),
                        source_quality: SourceQuality::OfficialStructured,
                        observed_via: Some("zai_coding_plan".into()),
                    },
                });
            }
        }

        if let (Some(rem), Some(limit)) =
            (stats.weekly_requests_remaining, stats.weekly_requests_limit)
        {
            if let (Ok(rem_dec), Ok(limit_dec)) =
                (DecimalValue::try_new(rem), DecimalValue::try_new(limit))
            {
                let remaining_percent = if limit > 0.0 {
                    Some((rem / limit) * 100.0)
                } else {
                    None
                };

                let resets_at = stats
                    .weekly_resets_at_unix
                    .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok());

                let window_id = WindowId::new("provider.zai.weekly")
                    .map_err(|_| SourceError::InvalidPayload)?;

                windows.push(BudgetWindow {
                    id: window_id,
                    label: Some("Weekly Quota".into()),
                    dimension: MetricDimension::Requests,
                    used: DecimalValue::try_new((limit - rem).max(0.0)).ok(),
                    limit: Some(limit_dec),
                    remaining: Some(rem_dec),
                    used_percent: remaining_percent.map(|r| (100.0 - r).clamp(0.0, 100.0)),
                    remaining_percent,
                    window_duration_seconds: Some(7 * 24 * 3600),
                    resets_at,
                    fresh_until: Some(now + SignedDuration::seconds(60)),
                    hard_blocked: rem <= 0.0,
                    applicability: Applicability::Applicable,
                    observed_at: now,
                    freshness: Freshness::Fresh,
                    provenance: Provenance {
                        source_id,
                        source_quality: SourceQuality::OfficialStructured,
                        observed_via: Some("zai_coding_plan".into()),
                    },
                });
            }
        }

        if windows.is_empty() {
            warnings.push(SnapshotWarning {
                code: "zai_stats_missing".into(),
                message: "No Z.AI coding plan quota information available".into(),
            });
        }

        Ok(BudgetSnapshot {
            source: descriptor,
            account_scope: None,
            availability: if windows.is_empty() {
                Availability::Unknown
            } else {
                Availability::Allowed
            },
            windows,
            observed_at: now,
            warnings,
        })
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
        if std::env::var_os("ZAI_API_KEY").is_some() || std::env::var_os("GLM_API_KEY").is_some() {
            Ok(ProbeReport {
                state: ProbeState::Ready,
            })
        } else {
            Ok(ProbeReport {
                state: ProbeState::NotAuthenticated,
            })
        }
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

        let weekly = &snapshot.windows[1];
        assert_eq!(weekly.dimension, MetricDimension::Requests);
        assert_eq!(weekly.remaining_percent, Some(92.0));
    }
    #[test]
    fn empty_stats_stay_unknown() {
        let snapshot = ZaiAdapter::normalize(ZaiCodingPlanStats::default()).expect("normalize");
        assert!(snapshot.windows.is_empty());
        assert!(matches!(snapshot.availability, Availability::Unknown));
    }

    #[tokio::test]
    async fn refresh_reports_unsupported_until_a_real_surface_exists() {
        let result = ZaiAdapter::new().refresh().await;
        assert!(matches!(result, Err(SourceError::UnsupportedVersion)));
    }
}
