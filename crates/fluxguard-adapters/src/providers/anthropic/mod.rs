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

impl AnthropicAdapter {
    pub fn new() -> Self {
        let id = SourceId::new("provider.anthropic").expect("static source id is valid");
        Self {
            descriptor: SourceDescriptor {
                id,
                kind: SourceKind::Provider,
                display_name: "Anthropic API".into(),
                adapter_version: env!("CARGO_PKG_VERSION").into(),
                source_quality: SourceQuality::OfficialHeaders,
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

    pub fn normalize(stats: AnthropicRateLimitStats) -> Result<BudgetSnapshot, SourceError> {
        let source_id =
            SourceId::new("provider.anthropic").map_err(|_| SourceError::InvalidPayload)?;
        let descriptor = SourceDescriptor {
            id: source_id.clone(),
            kind: SourceKind::Provider,
            display_name: "Anthropic API".into(),
            adapter_version: env!("CARGO_PKG_VERSION").into(),
            source_quality: SourceQuality::OfficialHeaders,
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

        if let (Some(rem), Some(limit)) = (stats.requests_remaining, stats.requests_limit) {
            if let (Ok(rem_dec), Ok(limit_dec)) =
                (DecimalValue::try_new(rem), DecimalValue::try_new(limit))
            {
                let remaining_percent = if limit > 0.0 {
                    Some((rem / limit) * 100.0)
                } else {
                    None
                };

                let resets_at = stats
                    .requests_reset_unix
                    .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok());

                let window_id = WindowId::new("provider.anthropic.requests")
                    .map_err(|_| SourceError::InvalidPayload)?;

                windows.push(BudgetWindow {
                    id: window_id,
                    label: Some("Requests Limit".into()),
                    dimension: MetricDimension::Requests,
                    used: DecimalValue::try_new((limit - rem).max(0.0)).ok(),
                    limit: Some(limit_dec),
                    remaining: Some(rem_dec),
                    used_percent: remaining_percent.map(|r| (100.0 - r).clamp(0.0, 100.0)),
                    remaining_percent,
                    window_duration_seconds: Some(60),
                    resets_at,
                    fresh_until: Some(now + SignedDuration::seconds(30)),
                    hard_blocked: rem <= 0.0,
                    applicability: Applicability::Applicable,
                    observed_at: now,
                    freshness: Freshness::Fresh,
                    provenance: Provenance {
                        source_id: source_id.clone(),
                        source_quality: SourceQuality::OfficialHeaders,
                        observed_via: Some("anthropic_rate_limit_headers".into()),
                    },
                });
            }
        }

        if let (Some(rem), Some(limit)) = (stats.input_tokens_remaining, stats.input_tokens_limit) {
            if let (Ok(rem_dec), Ok(limit_dec)) =
                (DecimalValue::try_new(rem), DecimalValue::try_new(limit))
            {
                let remaining_percent = if limit > 0.0 {
                    Some((rem / limit) * 100.0)
                } else {
                    None
                };

                let resets_at = stats
                    .input_tokens_reset_unix
                    .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok());

                let window_id = WindowId::new("provider.anthropic.input_tokens")
                    .map_err(|_| SourceError::InvalidPayload)?;

                windows.push(BudgetWindow {
                    id: window_id,
                    label: Some("Input Tokens per Minute".into()),
                    dimension: MetricDimension::InputTokens,
                    used: DecimalValue::try_new((limit - rem).max(0.0)).ok(),
                    limit: Some(limit_dec),
                    remaining: Some(rem_dec),
                    used_percent: remaining_percent.map(|r| (100.0 - r).clamp(0.0, 100.0)),
                    remaining_percent,
                    window_duration_seconds: Some(60),
                    resets_at,
                    fresh_until: Some(now + SignedDuration::seconds(30)),
                    hard_blocked: rem <= 0.0,
                    applicability: Applicability::Applicable,
                    observed_at: now,
                    freshness: Freshness::Fresh,
                    provenance: Provenance {
                        source_id: source_id.clone(),
                        source_quality: SourceQuality::OfficialHeaders,
                        observed_via: Some("anthropic_rate_limit_headers".into()),
                    },
                });
            }
        }

        if let (Some(rem), Some(limit)) = (stats.output_tokens_remaining, stats.output_tokens_limit)
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
                    .output_tokens_reset_unix
                    .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok());

                let window_id = WindowId::new("provider.anthropic.output_tokens")
                    .map_err(|_| SourceError::InvalidPayload)?;

                windows.push(BudgetWindow {
                    id: window_id,
                    label: Some("Output Tokens per Minute".into()),
                    dimension: MetricDimension::OutputTokens,
                    used: DecimalValue::try_new((limit - rem).max(0.0)).ok(),
                    limit: Some(limit_dec),
                    remaining: Some(rem_dec),
                    used_percent: remaining_percent.map(|r| (100.0 - r).clamp(0.0, 100.0)),
                    remaining_percent,
                    window_duration_seconds: Some(60),
                    resets_at,
                    fresh_until: Some(now + SignedDuration::seconds(30)),
                    hard_blocked: rem <= 0.0,
                    applicability: Applicability::Applicable,
                    observed_at: now,
                    freshness: Freshness::Fresh,
                    provenance: Provenance {
                        source_id,
                        source_quality: SourceQuality::OfficialHeaders,
                        observed_via: Some("anthropic_rate_limit_headers".into()),
                    },
                });
            }
        }

        if windows.is_empty() {
            warnings.push(SnapshotWarning {
                code: "anthropic_stats_missing".into(),
                message: "No Anthropic rate limit headers available".into(),
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
        if std::env::var_os("ANTHROPIC_API_KEY").is_some() {
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
    fn anthropic_stats_normalizes_requests_and_token_windows() {
        let stats = AnthropicRateLimitStats {
            requests_limit: Some(5_000.0),
            requests_remaining: Some(4_200.0),
            requests_reset_unix: Some(1774915200),
            input_tokens_limit: Some(400_000.0),
            input_tokens_remaining: Some(360_000.0),
            input_tokens_reset_unix: Some(1774915200),
            output_tokens_limit: Some(80_000.0),
            output_tokens_remaining: Some(72_000.0),
            output_tokens_reset_unix: Some(1774915200),
        };

        let snapshot = AnthropicAdapter::normalize(stats).expect("normalize");
        assert_eq!(snapshot.windows.len(), 3);

        let req_window = &snapshot.windows[0];
        assert_eq!(req_window.dimension, MetricDimension::Requests);
        assert_eq!(req_window.remaining_percent, Some(84.0));

        let in_tok = &snapshot.windows[1];
        assert_eq!(in_tok.dimension, MetricDimension::InputTokens);
        assert_eq!(in_tok.remaining_percent, Some(90.0));

        let out_tok = &snapshot.windows[2];
        assert_eq!(out_tok.dimension, MetricDimension::OutputTokens);
        assert_eq!(out_tok.remaining_percent, Some(90.0));
    }
    #[test]
    fn empty_stats_stay_unknown() {
        let snapshot =
            AnthropicAdapter::normalize(AnthropicRateLimitStats::default()).expect("normalize");
        assert!(snapshot.windows.is_empty());
        assert!(matches!(snapshot.availability, Availability::Unknown));
    }

    #[tokio::test]
    async fn refresh_reports_unsupported_until_a_real_surface_exists() {
        let result = AnthropicAdapter::new().refresh().await;
        assert!(matches!(result, Err(SourceError::UnsupportedVersion)));
    }
}
