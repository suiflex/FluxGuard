use ::time::{Duration as SignedDuration, OffsetDateTime};
use async_trait::async_trait;
use fluxguard_core::{
    BudgetSnapshot, MetricDimension, SourceDescriptor, SourceKind, SourceQuality,
};
use fluxguard_runtime::{BudgetSource, ProbeReport, SourceError};
use serde::Deserialize;

use crate::support::{self, WindowSpec};

const RATE_LIMITS_VIA: &str = "openai_rate_limits";
const BUDGET_VIA: &str = "openai_usage_budget";

/// OpenAI API rate limit and budget adapter.
#[derive(Clone, Debug)]
pub struct OpenAiAdapter {
    descriptor: SourceDescriptor,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiRateLimitStats {
    #[serde(default)]
    pub requests_limit: Option<f64>,
    #[serde(default)]
    pub requests_remaining: Option<f64>,
    #[serde(default)]
    pub requests_reset_seconds: Option<f64>,
    #[serde(default)]
    pub tokens_limit: Option<f64>,
    #[serde(default)]
    pub tokens_remaining: Option<f64>,
    #[serde(default)]
    pub tokens_reset_seconds: Option<f64>,
    #[serde(default)]
    pub current_spend_usd: Option<f64>,
    #[serde(default)]
    pub spend_limit_usd: Option<f64>,
}

fn descriptor() -> SourceDescriptor {
    support::descriptor(
        "provider.openai",
        SourceKind::Provider,
        "OpenAI API",
        SourceQuality::OfficialHeaders,
        support::capabilities(true, true),
    )
}

fn resets_in(now: OffsetDateTime, seconds: Option<f64>) -> Option<OffsetDateTime> {
    seconds.map(|secs| now + SignedDuration::milliseconds((secs * 1000.0) as i64))
}

impl OpenAiAdapter {
    pub fn new() -> Self {
        Self {
            descriptor: descriptor(),
        }
    }

    pub fn normalize(stats: OpenAiRateLimitStats) -> Result<BudgetSnapshot, SourceError> {
        let source = descriptor();
        let now = OffsetDateTime::now_utc();
        let mut windows = Vec::new();

        let per_minute = [
            (
                "provider.openai.requests",
                "Requests per Minute",
                MetricDimension::Requests,
                stats.requests_remaining,
                stats.requests_limit,
                stats.requests_reset_seconds,
            ),
            (
                "provider.openai.tokens",
                "Tokens per Minute",
                MetricDimension::Tokens,
                stats.tokens_remaining,
                stats.tokens_limit,
                stats.tokens_reset_seconds,
            ),
        ];
        for (id, label, dimension, remaining, limit, reset_seconds) in per_minute {
            if remaining.is_none() || limit.is_none() {
                continue;
            }
            support::push_window(
                &mut windows,
                &source,
                RATE_LIMITS_VIA,
                now,
                WindowSpec {
                    remaining,
                    limit,
                    window_duration_seconds: Some(60),
                    resets_at: resets_in(now, reset_seconds),
                    ttl_seconds: 30,
                    ..WindowSpec::new(id, label, dimension)
                },
            )?;
        }

        if let (Some(spend), Some(limit)) = (stats.current_spend_usd, stats.spend_limit_usd) {
            support::push_window(
                &mut windows,
                &source,
                BUDGET_VIA,
                now,
                WindowSpec {
                    used: Some(spend),
                    limit: Some(limit),
                    ..WindowSpec::new(
                        "provider.openai.spend",
                        "Usage Budget Spend",
                        MetricDimension::Currency,
                    )
                },
            )?;
        }

        Ok(support::snapshot(
            source,
            now,
            windows,
            "openai_stats_missing",
            "No OpenAI rate limits or budget telemetry available",
        ))
    }
}

impl Default for OpenAiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BudgetSource for OpenAiAdapter {
    fn descriptor(&self) -> SourceDescriptor {
        self.descriptor.clone()
    }

    async fn probe(&self) -> Result<ProbeReport, SourceError> {
        Ok(support::probe_env_keys(&["OPENAI_API_KEY"]))
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
    fn openai_stats_normalizes_requests_and_tokens_limits() {
        // Shape is FluxGuard's own normalized contract, not a vendor payload.
        let stats: OpenAiRateLimitStats = serde_json::from_str(include_str!(
            "../../../tests/fixtures/openai/rate_limits_basic.json"
        ))
        .expect("fixture");

        let snapshot = OpenAiAdapter::normalize(stats).expect("normalize");
        assert_eq!(snapshot.windows.len(), 3);

        let req_window = &snapshot.windows[0];
        assert_eq!(req_window.dimension, MetricDimension::Requests);
        assert_eq!(req_window.remaining_percent, Some(85.0));
        assert!(req_window.resets_at.is_some());

        let tok_window = &snapshot.windows[1];
        assert_eq!(tok_window.dimension, MetricDimension::Tokens);
        assert_eq!(tok_window.remaining_percent, Some(90.0));

        let spend_window = &snapshot.windows[2];
        assert_eq!(spend_window.dimension, MetricDimension::Currency);
        assert_eq!(spend_window.remaining_percent, Some(84.8));
    }

    detection_only_contract!(OpenAiAdapter, OpenAiAdapter::new(), OpenAiRateLimitStats);
}
