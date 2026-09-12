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

impl OpenAiAdapter {
    pub fn new() -> Self {
        let id = SourceId::new("provider.openai").expect("static source id is valid");
        Self {
            descriptor: SourceDescriptor {
                id,
                kind: SourceKind::Provider,
                display_name: "OpenAI API".into(),
                adapter_version: env!("CARGO_PKG_VERSION").into(),
                source_quality: SourceQuality::OfficialHeaders,
                capabilities: SourceCapabilities {
                    supports_snapshot: true,
                    supports_push_updates: false,
                    supports_reset_time: true,
                    supports_exact_remaining_percent: true,
                    supports_model_scope: true,
                    supports_cost: true,
                },
            },
        }
    }

    pub fn normalize(stats: OpenAiRateLimitStats) -> Result<BudgetSnapshot, SourceError> {
        let source_id =
            SourceId::new("provider.openai").map_err(|_| SourceError::InvalidPayload)?;
        let descriptor = SourceDescriptor {
            id: source_id.clone(),
            kind: SourceKind::Provider,
            display_name: "OpenAI API".into(),
            adapter_version: env!("CARGO_PKG_VERSION").into(),
            source_quality: SourceQuality::OfficialHeaders,
            capabilities: SourceCapabilities {
                supports_snapshot: true,
                supports_push_updates: false,
                supports_reset_time: true,
                supports_exact_remaining_percent: true,
                supports_model_scope: true,
                supports_cost: true,
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
                    .requests_reset_seconds
                    .map(|secs| now + SignedDuration::milliseconds((secs * 1000.0) as i64));

                let window_id = WindowId::new("provider.openai.requests")
                    .map_err(|_| SourceError::InvalidPayload)?;

                windows.push(BudgetWindow {
                    id: window_id,
                    label: Some("Requests per Minute".into()),
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
                        observed_via: Some("openai_rate_limits".into()),
                    },
                });
            }
        }

        if let (Some(rem), Some(limit)) = (stats.tokens_remaining, stats.tokens_limit) {
            if let (Ok(rem_dec), Ok(limit_dec)) =
                (DecimalValue::try_new(rem), DecimalValue::try_new(limit))
            {
                let remaining_percent = if limit > 0.0 {
                    Some((rem / limit) * 100.0)
                } else {
                    None
                };

                let resets_at = stats
                    .tokens_reset_seconds
                    .map(|secs| now + SignedDuration::milliseconds((secs * 1000.0) as i64));

                let window_id = WindowId::new("provider.openai.tokens")
                    .map_err(|_| SourceError::InvalidPayload)?;

                windows.push(BudgetWindow {
                    id: window_id,
                    label: Some("Tokens per Minute".into()),
                    dimension: MetricDimension::Tokens,
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
                        observed_via: Some("openai_rate_limits".into()),
                    },
                });
            }
        }

        if let (Some(spend), Some(limit)) = (stats.current_spend_usd, stats.spend_limit_usd) {
            if let (Ok(spend_dec), Ok(limit_dec)) =
                (DecimalValue::try_new(spend), DecimalValue::try_new(limit))
            {
                let remaining_val = (limit - spend).max(0.0);
                let remaining_percent = if limit > 0.0 {
                    Some((remaining_val / limit) * 100.0)
                } else {
                    None
                };

                let window_id = WindowId::new("provider.openai.spend")
                    .map_err(|_| SourceError::InvalidPayload)?;

                windows.push(BudgetWindow {
                    id: window_id,
                    label: Some("Usage Budget Spend".into()),
                    dimension: MetricDimension::Currency,
                    used: Some(spend_dec),
                    limit: Some(limit_dec),
                    remaining: DecimalValue::try_new(remaining_val).ok(),
                    used_percent: remaining_percent.map(|r| (100.0 - r).clamp(0.0, 100.0)),
                    remaining_percent,
                    window_duration_seconds: None,
                    resets_at: None,
                    fresh_until: Some(now + SignedDuration::seconds(60)),
                    hard_blocked: remaining_val <= 0.0,
                    applicability: Applicability::Applicable,
                    observed_at: now,
                    freshness: Freshness::Fresh,
                    provenance: Provenance {
                        source_id,
                        source_quality: SourceQuality::OfficialHeaders,
                        observed_via: Some("openai_usage_budget".into()),
                    },
                });
            }
        }

        if windows.is_empty() {
            warnings.push(SnapshotWarning {
                code: "openai_stats_missing".into(),
                message: "No OpenAI rate limits or budget telemetry available".into(),
            });
        }

        Ok(BudgetSnapshot {
            source: descriptor,
            account_scope: None,
            availability: Availability::Allowed,
            windows,
            observed_at: now,
            warnings,
        })
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
        if std::env::var_os("OPENAI_API_KEY").is_some() {
            Ok(ProbeReport {
                state: ProbeState::Ready,
            })
        } else {
            Ok(ProbeReport {
                state: ProbeState::NotAuthenticated,
            })
        }
    }

    async fn refresh(&self) -> Result<BudgetSnapshot, SourceError> {
        Self::normalize(OpenAiRateLimitStats::default())
    }

    async fn run(
        &self,
        updates: watch::Sender<fluxguard_runtime::SourceState>,
        cancel: CancellationToken,
    ) -> Result<(), SourceError> {
        let snapshot = self.refresh().await?;
        updates
            .send(fluxguard_runtime::SourceState::ready(snapshot))
            .map_err(|_| SourceError::Other)?;
        cancel.cancelled().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_stats_normalizes_requests_and_tokens_limits() {
        let stats = OpenAiRateLimitStats {
            requests_limit: Some(10_000.0),
            requests_remaining: Some(8_500.0),
            requests_reset_seconds: Some(0.12),
            tokens_limit: Some(2_000_000.0),
            tokens_remaining: Some(1_800_000.0),
            tokens_reset_seconds: Some(0.45),
            current_spend_usd: Some(15.20),
            spend_limit_usd: Some(100.0),
        };

        let snapshot = OpenAiAdapter::normalize(stats).expect("normalize");
        assert_eq!(snapshot.windows.len(), 3);

        let req_window = &snapshot.windows[0];
        assert_eq!(req_window.dimension, MetricDimension::Requests);
        assert_eq!(req_window.remaining_percent, Some(85.0));

        let tok_window = &snapshot.windows[1];
        assert_eq!(tok_window.dimension, MetricDimension::Tokens);
        assert_eq!(tok_window.remaining_percent, Some(90.0));

        let spend_window = &snapshot.windows[2];
        assert_eq!(spend_window.dimension, MetricDimension::Currency);
        assert_eq!(spend_window.remaining_percent, Some(84.8));
    }
}
