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

/// Cursor agent lifecycle and model pool telemetry adapter.
#[derive(Clone, Debug)]
pub struct CursorAdapter {
    descriptor: SourceDescriptor,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorUsageStats {
    #[serde(default)]
    pub fast_requests_used: Option<f64>,
    #[serde(default)]
    pub fast_requests_limit: Option<f64>,
    #[serde(default)]
    pub slow_requests_used: Option<f64>,
    #[serde(default)]
    pub tokens_used: Option<f64>,
    #[serde(default)]
    pub monthly_spend_usd: Option<f64>,
}

impl CursorAdapter {
    pub fn new() -> Self {
        let id = SourceId::new("client.cursor").expect("static source id is valid");
        Self {
            descriptor: SourceDescriptor {
                id,
                kind: SourceKind::Client,
                display_name: "Cursor".into(),
                adapter_version: env!("CARGO_PKG_VERSION").into(),
                source_quality: SourceQuality::OfficialTelemetry,
                capabilities: SourceCapabilities {
                    supports_snapshot: true,
                    supports_push_updates: false,
                    supports_reset_time: false,
                    supports_exact_remaining_percent: true,
                    supports_model_scope: true,
                    supports_cost: true,
                },
            },
        }
    }

    pub fn normalize(stats: CursorUsageStats) -> Result<BudgetSnapshot, SourceError> {
        let source_id = SourceId::new("client.cursor").map_err(|_| SourceError::InvalidPayload)?;
        let descriptor = SourceDescriptor {
            id: source_id.clone(),
            kind: SourceKind::Client,
            display_name: "Cursor".into(),
            adapter_version: env!("CARGO_PKG_VERSION").into(),
            source_quality: SourceQuality::OfficialTelemetry,
            capabilities: SourceCapabilities {
                supports_snapshot: true,
                supports_push_updates: false,
                supports_reset_time: false,
                supports_exact_remaining_percent: true,
                supports_model_scope: true,
                supports_cost: true,
            },
        };

        let now = OffsetDateTime::now_utc();
        let mut windows = Vec::new();
        let mut warnings = Vec::new();

        if let (Some(used_val), Some(limit_val)) =
            (stats.fast_requests_used, stats.fast_requests_limit)
        {
            if let (Ok(used_dec), Ok(limit_dec)) = (
                DecimalValue::try_new(used_val),
                DecimalValue::try_new(limit_val),
            ) {
                let remaining_val = (limit_val - used_val).max(0.0);
                let remaining_percent = if limit_val > 0.0 {
                    Some((remaining_val / limit_val) * 100.0)
                } else {
                    None
                };

                let window_id = WindowId::new("client.cursor.fast_requests")
                    .map_err(|_| SourceError::InvalidPayload)?;

                windows.push(BudgetWindow {
                    id: window_id,
                    label: Some("Fast Requests".into()),
                    dimension: MetricDimension::Requests,
                    used: Some(used_dec),
                    limit: Some(limit_dec),
                    remaining: DecimalValue::try_new(remaining_val).ok(),
                    used_percent: remaining_percent.map(|rem| (100.0 - rem).clamp(0.0, 100.0)),
                    remaining_percent,
                    window_duration_seconds: None,
                    resets_at: None,
                    fresh_until: Some(now + SignedDuration::seconds(60)),
                    hard_blocked: remaining_val <= 0.0,
                    applicability: Applicability::Applicable,
                    observed_at: now,
                    freshness: Freshness::Fresh,
                    provenance: Provenance {
                        source_id: source_id.clone(),
                        source_quality: SourceQuality::OfficialTelemetry,
                        observed_via: Some("cursor_telemetry".into()),
                    },
                });
            }
        }

        if let Some(tokens) = stats
            .tokens_used
            .and_then(|v| DecimalValue::try_new(v).ok())
        {
            if let Ok(window_id) = WindowId::new("client.cursor.tokens") {
                windows.push(BudgetWindow {
                    id: window_id,
                    label: Some("Total Tokens".into()),
                    dimension: MetricDimension::Tokens,
                    used: Some(tokens),
                    limit: None,
                    remaining: None,
                    used_percent: None,
                    remaining_percent: None,
                    window_duration_seconds: None,
                    resets_at: None,
                    fresh_until: Some(now + SignedDuration::seconds(60)),
                    hard_blocked: false,
                    applicability: Applicability::Applicable,
                    observed_at: now,
                    freshness: Freshness::Fresh,
                    provenance: Provenance {
                        source_id: source_id.clone(),
                        source_quality: SourceQuality::OfficialTelemetry,
                        observed_via: Some("cursor_telemetry".into()),
                    },
                });
            }
        }

        if let Some(spend) = stats
            .monthly_spend_usd
            .and_then(|v| DecimalValue::try_new(v).ok())
        {
            if let Ok(window_id) = WindowId::new("client.cursor.cost") {
                windows.push(BudgetWindow {
                    id: window_id,
                    label: Some("Monthly Spend".into()),
                    dimension: MetricDimension::Currency,
                    used: Some(spend),
                    limit: None,
                    remaining: None,
                    used_percent: None,
                    remaining_percent: None,
                    window_duration_seconds: None,
                    resets_at: None,
                    fresh_until: Some(now + SignedDuration::seconds(60)),
                    hard_blocked: false,
                    applicability: Applicability::Applicable,
                    observed_at: now,
                    freshness: Freshness::Fresh,
                    provenance: Provenance {
                        source_id,
                        source_quality: SourceQuality::OfficialTelemetry,
                        observed_via: Some("cursor_telemetry".into()),
                    },
                });
            }
        }

        if windows.is_empty() {
            warnings.push(SnapshotWarning {
                code: "cursor_stats_missing".into(),
                message: "No Cursor usage statistics available".into(),
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

impl Default for CursorAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BudgetSource for CursorAdapter {
    fn descriptor(&self) -> SourceDescriptor {
        self.descriptor.clone()
    }

    async fn probe(&self) -> Result<ProbeReport, SourceError> {
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let cursor_dir_exists = home.map(|h| h.join(".cursor").exists()).unwrap_or(false);

        if cursor_dir_exists || std::env::var_os("CURSOR_USER_ID").is_some() {
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
    fn cursor_stats_normalizes_fast_requests_window() {
        // Shape is FluxGuard's own normalized contract, not a vendor payload.
        let stats: CursorUsageStats = serde_json::from_str(include_str!(
            "../../../tests/fixtures/cursor/usage_basic.json"
        ))
        .expect("fixture");

        let snapshot = CursorAdapter::normalize(stats).expect("normalize");
        assert_eq!(snapshot.windows.len(), 3);

        let fast_window = &snapshot.windows[0];
        assert_eq!(fast_window.dimension, MetricDimension::Requests);
        assert_eq!(fast_window.used.map(|v| v.0), Some(40.0));
        assert_eq!(fast_window.limit.map(|v| v.0), Some(500.0));
        assert_eq!(fast_window.remaining_percent, Some(92.0));
    }
    #[test]
    fn empty_stats_stay_unknown() {
        let snapshot = CursorAdapter::normalize(CursorUsageStats::default()).expect("normalize");
        assert!(snapshot.windows.is_empty());
        assert!(matches!(snapshot.availability, Availability::Unknown));
    }

    #[tokio::test]
    async fn refresh_reports_unsupported_until_a_real_surface_exists() {
        let result = CursorAdapter::new().refresh().await;
        assert!(matches!(result, Err(SourceError::UnsupportedVersion)));
    }
}
