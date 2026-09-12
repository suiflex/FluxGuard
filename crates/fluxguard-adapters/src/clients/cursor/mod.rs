use ::time::OffsetDateTime;
use async_trait::async_trait;
use fluxguard_core::{
    BudgetSnapshot, MetricDimension, SourceDescriptor, SourceKind, SourceQuality,
};
use fluxguard_runtime::{BudgetSource, ProbeReport, ProbeState, SourceError};
use serde::Deserialize;

use crate::support::{self, WindowSpec};

const OBSERVED_VIA: &str = "cursor_telemetry";

/// Cursor agent lifecycle and model pool telemetry adapter.
#[derive(Clone, Debug)]
pub struct CursorAdapter;

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

fn descriptor() -> SourceDescriptor {
    support::descriptor(
        "client.cursor",
        SourceKind::Client,
        "Cursor",
        SourceQuality::OfficialTelemetry,
        support::capabilities(false, true),
    )
}

impl CursorAdapter {
    pub fn new() -> Self {
        Self
    }

    pub fn normalize(stats: CursorUsageStats) -> Result<BudgetSnapshot, SourceError> {
        let source = descriptor();
        let now = OffsetDateTime::now_utc();
        let mut windows = Vec::new();

        if let (Some(used), Some(limit)) = (stats.fast_requests_used, stats.fast_requests_limit) {
            support::push_window(
                &mut windows,
                &source,
                OBSERVED_VIA,
                now,
                WindowSpec {
                    used: Some(used),
                    limit: Some(limit),
                    ..WindowSpec::new(
                        "client.cursor.fast_requests",
                        "Fast Requests",
                        MetricDimension::Requests,
                    )
                },
            )?;
        }
        if stats.tokens_used.is_some() {
            support::push_window(
                &mut windows,
                &source,
                OBSERVED_VIA,
                now,
                WindowSpec {
                    used: stats.tokens_used,
                    ..WindowSpec::new(
                        "client.cursor.tokens",
                        "Total Tokens",
                        MetricDimension::Tokens,
                    )
                },
            )?;
        }
        if stats.monthly_spend_usd.is_some() {
            support::push_window(
                &mut windows,
                &source,
                OBSERVED_VIA,
                now,
                WindowSpec {
                    used: stats.monthly_spend_usd,
                    ..WindowSpec::new(
                        "client.cursor.cost",
                        "Monthly Spend",
                        MetricDimension::Currency,
                    )
                },
            )?;
        }

        Ok(support::snapshot(
            source,
            now,
            windows,
            "cursor_stats_missing",
            "No Cursor usage statistics available",
        ))
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
        descriptor()
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_stats_normalizes_fast_requests_window() {
        // Shape is FluxGuard's own normalized contract, not a vendor payload.
        let snapshot = crate::support::fixture_snapshot(
            include_str!("../../../tests/fixtures/cursor/usage_basic.json"),
            CursorAdapter::normalize,
        );
        assert_eq!(snapshot.windows.len(), 3);

        let fast_window = &snapshot.windows[0];
        assert_eq!(fast_window.dimension, MetricDimension::Requests);
        assert_eq!(fast_window.used.map(|v| v.0), Some(40.0));
        assert_eq!(fast_window.limit.map(|v| v.0), Some(500.0));
        assert_eq!(fast_window.remaining_percent, Some(92.0));
    }

    detection_only_contract!(CursorAdapter, CursorAdapter::new(), CursorUsageStats);
}
