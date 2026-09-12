use ::time::{Duration as SignedDuration, OffsetDateTime};
use async_trait::async_trait;
use fluxguard_core::{
    BudgetSnapshot, MetricDimension, SourceDescriptor, SourceKind, SourceQuality,
};
use fluxguard_runtime::{BudgetSource, ProbeReport, SourceError};
use serde::Deserialize;

use crate::support::{self, WindowSpec};

const OBSERVED_VIA: &str = "xai_rate_limits";

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

fn descriptor() -> SourceDescriptor {
    support::descriptor(
        "provider.xai",
        SourceKind::Provider,
        "xAI API",
        SourceQuality::Estimated,
        support::capabilities(true, false),
    )
}

impl XaiAdapter {
    pub fn new() -> Self {
        Self {
            descriptor: descriptor(),
        }
    }

    pub fn normalize(stats: XaiRateLimitStats) -> Result<BudgetSnapshot, SourceError> {
        let source = descriptor();
        let now = OffsetDateTime::now_utc();
        let mut windows = Vec::new();

        if let (Some(used), Some(limit)) = (
            stats.requests_per_second_used,
            stats.requests_per_second_limit,
        ) {
            support::push_window(
                &mut windows,
                &source,
                OBSERVED_VIA,
                now,
                WindowSpec {
                    used: Some(used),
                    limit: Some(limit),
                    window_duration_seconds: Some(1),
                    resets_at: stats
                        .retry_after_seconds
                        .map(|secs| now + SignedDuration::milliseconds((secs * 1000.0) as i64)),
                    ttl_seconds: 5,
                    ..WindowSpec::new(
                        "provider.xai.requests_per_second",
                        "Requests per Second",
                        MetricDimension::Requests,
                    )
                },
            )?;
        }

        if let (Some(used), Some(limit)) =
            (stats.tokens_per_minute_used, stats.tokens_per_minute_limit)
        {
            support::push_window(
                &mut windows,
                &source,
                OBSERVED_VIA,
                now,
                WindowSpec {
                    used: Some(used),
                    limit: Some(limit),
                    window_duration_seconds: Some(60),
                    ttl_seconds: 30,
                    ..WindowSpec::new(
                        "provider.xai.tokens_per_minute",
                        "Tokens per Minute",
                        MetricDimension::Tokens,
                    )
                },
            )?;
        }

        Ok(support::snapshot(
            source,
            now,
            windows,
            "xai_stats_missing",
            "No xAI rate limit statistics available",
        ))
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
        Ok(support::probe_env_keys(&["XAI_API_KEY"]))
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
        assert_eq!(rps.window_duration_seconds, Some(1));

        let tpm = &snapshot.windows[1];
        assert_eq!(tpm.dimension, MetricDimension::Tokens);
        assert_eq!(tpm.remaining_percent, Some(80.0));
    }

    detection_only_contract!(XaiAdapter, XaiAdapter::new(), XaiRateLimitStats);
}
