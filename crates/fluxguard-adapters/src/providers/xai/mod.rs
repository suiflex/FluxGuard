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

/// xAI (Grok) API rate limit adapter.
#[derive(Clone, Debug)]
pub struct XaiAdapter {
    descriptor: SourceDescriptor,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XaiRateLimitStats {
    #[serde(default)]
    pub requests_per_second_limit: Option<f64>,
    #[serde(default)]
    pub requests_per_second_used: Option<f64>,
    #[serde(default)]
    pub tokens_per_minute_limit: Option<f64>,
    #[serde(default)]
    pub tokens_per_minute_used: Option<f64>,
    #[serde(default)]
    pub retry_after_seconds: Option<f64>,
}

impl XaiAdapter {
    pub fn new() -> Self {
        let id = SourceId::new("provider.xai").expect("static source id is valid");
        Self {
            descriptor: SourceDescriptor {
                id,
                kind: SourceKind::Provider,
                display_name: "xAI API".into(),
                adapter_version: env!("CARGO_PKG_VERSION").into(),
                source_quality: SourceQuality::Estimated,
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

    pub fn normalize(stats: XaiRateLimitStats) -> Result<BudgetSnapshot, SourceError> {
        let source_id = SourceId::new("provider.xai").map_err(|_| SourceError::InvalidPayload)?;
        let descriptor = SourceDescriptor {
            id: source_id.clone(),
            kind: SourceKind::Provider,
            display_name: "xAI API".into(),
            adapter_version: env!("CARGO_PKG_VERSION").into(),
            source_quality: SourceQuality::Estimated,
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

        if let (Some(used), Some(limit)) = (
            stats.requests_per_second_used,
            stats.requests_per_second_limit,
        ) {
            if let (Ok(used_dec), Ok(limit_dec)) =
                (DecimalValue::try_new(used), DecimalValue::try_new(limit))
            {
                let remaining_val = (limit - used).max(0.0);
                let remaining_percent = if limit > 0.0 {
                    Some((remaining_val / limit) * 100.0)
                } else {
                    None
                };

                let resets_at = stats
                    .retry_after_seconds
                    .map(|secs| now + SignedDuration::milliseconds((secs * 1000.0) as i64));

                let window_id = WindowId::new("provider.xai.requests_per_second")
                    .map_err(|_| SourceError::InvalidPayload)?;

                windows.push(BudgetWindow {
                    id: window_id,
                    label: Some("Requests per Second".into()),
                    dimension: MetricDimension::Requests,
                    used: Some(used_dec),
                    limit: Some(limit_dec),
                    remaining: DecimalValue::try_new(remaining_val).ok(),
                    used_percent: remaining_percent.map(|r| (100.0 - r).clamp(0.0, 100.0)),
                    remaining_percent,
                    window_duration_seconds: Some(1),
                    resets_at,
                    fresh_until: Some(now + SignedDuration::seconds(5)),
                    hard_blocked: remaining_val <= 0.0,
                    applicability: Applicability::Applicable,
                    observed_at: now,
                    freshness: Freshness::Fresh,
                    provenance: Provenance {
                        source_id: source_id.clone(),
                        source_quality: SourceQuality::Estimated,
                        observed_via: Some("xai_rate_limits".into()),
                    },
                });
            }
        }

        if let (Some(used), Some(limit)) =
            (stats.tokens_per_minute_used, stats.tokens_per_minute_limit)
        {
            if let (Ok(used_dec), Ok(limit_dec)) =
                (DecimalValue::try_new(used), DecimalValue::try_new(limit))
            {
                let remaining_val = (limit - used).max(0.0);
                let remaining_percent = if limit > 0.0 {
                    Some((remaining_val / limit) * 100.0)
                } else {
                    None
                };

                let window_id = WindowId::new("provider.xai.tokens_per_minute")
                    .map_err(|_| SourceError::InvalidPayload)?;

                windows.push(BudgetWindow {
                    id: window_id,
                    label: Some("Tokens per Minute".into()),
                    dimension: MetricDimension::Tokens,
                    used: Some(used_dec),
                    limit: Some(limit_dec),
                    remaining: DecimalValue::try_new(remaining_val).ok(),
                    used_percent: remaining_percent.map(|r| (100.0 - r).clamp(0.0, 100.0)),
                    remaining_percent,
                    window_duration_seconds: Some(60),
                    resets_at: None,
                    fresh_until: Some(now + SignedDuration::seconds(30)),
                    hard_blocked: remaining_val <= 0.0,
                    applicability: Applicability::Applicable,
                    observed_at: now,
                    freshness: Freshness::Fresh,
                    provenance: Provenance {
                        source_id,
                        source_quality: SourceQuality::Estimated,
                        observed_via: Some("xai_rate_limits".into()),
                    },
                });
            }
        }

        if windows.is_empty() {
            warnings.push(SnapshotWarning {
                code: "xai_stats_missing".into(),
                message: "No xAI rate limit statistics available".into(),
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

impl Default for XaiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BudgetSource for XaiAdapter {
    fn descriptor(&self) -> SourceDescriptor {
        self.descriptor.clone()
    }

    async fn probe(&self) -> Result<ProbeReport, SourceError> {
        if std::env::var_os("XAI_API_KEY").is_some() {
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
    fn xai_stats_normalizes_rps_and_tpm_limits() {
        // Shape is FluxGuard's own normalized contract, not a vendor payload.
        let stats: XaiRateLimitStats = serde_json::from_str(include_str!(
            "../../../tests/fixtures/xai/rate_limits_basic.json"
        ))
        .expect("fixture");

        let snapshot = XaiAdapter::normalize(stats).expect("normalize");
        assert_eq!(snapshot.windows.len(), 2);

        let rps = &snapshot.windows[0];
        assert_eq!(rps.dimension, MetricDimension::Requests);
        assert_eq!(rps.remaining_percent, Some(80.0));

        let tpm = &snapshot.windows[1];
        assert_eq!(tpm.dimension, MetricDimension::Tokens);
        assert_eq!(tpm.remaining_percent, Some(80.0));
    }
    #[test]
    fn empty_stats_stay_unknown() {
        let snapshot = XaiAdapter::normalize(XaiRateLimitStats::default()).expect("normalize");
        assert!(snapshot.windows.is_empty());
        assert!(matches!(snapshot.availability, Availability::Unknown));
    }

    #[tokio::test]
    async fn refresh_reports_unsupported_until_a_real_surface_exists() {
        let result = XaiAdapter::new().refresh().await;
        assert!(matches!(result, Err(SourceError::UnsupportedVersion)));
    }
}
