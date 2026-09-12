use ::time::OffsetDateTime;
use async_trait::async_trait;
use fluxguard_core::{
    BudgetSnapshot, MetricDimension, SourceDescriptor, SourceKind, SourceQuality,
};
use fluxguard_runtime::{BudgetSource, ProbeReport, SourceError};
use serde::Deserialize;

use crate::support::{self, WindowSpec};

const OBSERVED_VIA: &str = "anthropic_rate_limit_headers";

/// Anthropic API rate limit adapter.
#[derive(Clone, Debug)]
pub struct AnthropicAdapter {
    descriptor: SourceDescriptor,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnthropicRateLimitStats {
    #[serde(default)]
    pub requests_limit: Option<f64>,
    #[serde(default)]
    pub requests_remaining: Option<f64>,
    #[serde(default)]
    pub requests_reset_unix: Option<i64>,
    #[serde(default)]
    pub input_tokens_limit: Option<f64>,
    #[serde(default)]
    pub input_tokens_remaining: Option<f64>,
    #[serde(default)]
    pub input_tokens_reset_unix: Option<i64>,
    #[serde(default)]
    pub output_tokens_limit: Option<f64>,
    #[serde(default)]
    pub output_tokens_remaining: Option<f64>,
    #[serde(default)]
    pub output_tokens_reset_unix: Option<i64>,
}

fn descriptor() -> SourceDescriptor {
    support::descriptor(
        "provider.anthropic",
        SourceKind::Provider,
        "Anthropic API",
        SourceQuality::OfficialHeaders,
        support::capabilities(true, false),
    )
}

impl AnthropicAdapter {
    pub fn new() -> Self {
        Self {
            descriptor: descriptor(),
        }
    }

    pub fn normalize(stats: AnthropicRateLimitStats) -> Result<BudgetSnapshot, SourceError> {
        let source = descriptor();
        let now = OffsetDateTime::now_utc();
        let mut windows = Vec::new();

        let per_minute = [
            (
                "provider.anthropic.requests",
                "Requests Limit",
                MetricDimension::Requests,
                stats.requests_remaining,
                stats.requests_limit,
                stats.requests_reset_unix,
            ),
            (
                "provider.anthropic.input_tokens",
                "Input Tokens per Minute",
                MetricDimension::InputTokens,
                stats.input_tokens_remaining,
                stats.input_tokens_limit,
                stats.input_tokens_reset_unix,
            ),
            (
                "provider.anthropic.output_tokens",
                "Output Tokens per Minute",
                MetricDimension::OutputTokens,
                stats.output_tokens_remaining,
                stats.output_tokens_limit,
                stats.output_tokens_reset_unix,
            ),
        ];
        for (id, label, dimension, remaining, limit, reset_unix) in per_minute {
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
                    window_duration_seconds: Some(60),
                    resets_at: reset_unix
                        .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok()),
                    ttl_seconds: 30,
                    ..WindowSpec::new(id, label, dimension)
                },
            )?;
        }

        Ok(support::snapshot(
            source,
            now,
            windows,
            "anthropic_stats_missing",
            "No Anthropic rate limit headers available",
        ))
    }
}

impl Default for AnthropicAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BudgetSource for AnthropicAdapter {
    fn descriptor(&self) -> SourceDescriptor {
        self.descriptor.clone()
    }

    async fn probe(&self) -> Result<ProbeReport, SourceError> {
        Ok(support::probe_env_keys(&["ANTHROPIC_API_KEY"]))
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
    fn anthropic_stats_normalizes_requests_and_token_windows() {
        // Shape is FluxGuard's own normalized contract, not a vendor payload.
        let stats: AnthropicRateLimitStats = serde_json::from_str(include_str!(
            "../../../tests/fixtures/anthropic/rate_limits_basic.json"
        ))
        .expect("fixture");

        let snapshot = AnthropicAdapter::normalize(stats).expect("normalize");
        assert_eq!(snapshot.windows.len(), 3);

        let req_window = &snapshot.windows[0];
        assert_eq!(req_window.dimension, MetricDimension::Requests);
        assert_eq!(req_window.remaining_percent, Some(84.0));
        assert!(req_window.resets_at.is_some());

        let in_tok = &snapshot.windows[1];
        assert_eq!(in_tok.dimension, MetricDimension::InputTokens);
        assert_eq!(in_tok.remaining_percent, Some(90.0));

        let out_tok = &snapshot.windows[2];
        assert_eq!(out_tok.dimension, MetricDimension::OutputTokens);
        assert_eq!(out_tok.remaining_percent, Some(90.0));
    }

    detection_only_contract!(
        AnthropicAdapter,
        AnthropicAdapter::new(),
        AnthropicRateLimitStats
    );
}
