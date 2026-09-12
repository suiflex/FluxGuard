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

/// Claude Code client telemetry adapter.
#[derive(Clone, Debug)]
pub struct ClaudeCodeAdapter {
    command: String,
    descriptor: SourceDescriptor,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeCodeStats {
    #[serde(default)]
    pub session_input_tokens: Option<f64>,
    #[serde(default)]
    pub session_output_tokens: Option<f64>,
    #[serde(default)]
    pub context_tokens: Option<f64>,
    #[serde(default)]
    pub context_limit: Option<f64>,
}

impl ClaudeCodeAdapter {
    pub fn new(command: impl Into<String>) -> Self {
        let id = SourceId::new("client.claude_code").expect("static source id is valid");
        Self {
            command: command.into(),
            descriptor: SourceDescriptor {
                id,
                kind: SourceKind::Client,
                display_name: "Claude Code".into(),
                adapter_version: env!("CARGO_PKG_VERSION").into(),
                source_quality: SourceQuality::OfficialTelemetry,
                capabilities: SourceCapabilities {
                    supports_snapshot: true,
                    supports_push_updates: false,
                    supports_reset_time: false,
                    supports_exact_remaining_percent: true,
                    supports_model_scope: true,
                    supports_cost: false,
                },
            },
        }
    }

    pub fn normalize(stats: ClaudeCodeStats) -> Result<BudgetSnapshot, SourceError> {
        let source_id =
            SourceId::new("client.claude_code").map_err(|_| SourceError::InvalidPayload)?;
        let descriptor = SourceDescriptor {
            id: source_id.clone(),
            kind: SourceKind::Client,
            display_name: "Claude Code".into(),
            adapter_version: env!("CARGO_PKG_VERSION").into(),
            source_quality: SourceQuality::OfficialTelemetry,
            capabilities: SourceCapabilities {
                supports_snapshot: true,
                supports_push_updates: false,
                supports_reset_time: false,
                supports_exact_remaining_percent: true,
                supports_model_scope: true,
                supports_cost: false,
            },
        };

        let now = OffsetDateTime::now_utc();
        let mut windows = Vec::new();
        let mut warnings = Vec::new();

        if let (Some(used_val), Some(limit_val)) = (stats.context_tokens, stats.context_limit) {
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

                let window_id = WindowId::new("client.claude_code.context_window")
                    .map_err(|_| SourceError::InvalidPayload)?;

                windows.push(BudgetWindow {
                    id: window_id,
                    label: Some("Context Window".into()),
                    dimension: MetricDimension::ContextTokens,
                    used: Some(used_dec),
                    limit: Some(limit_dec),
                    remaining: DecimalValue::try_new(remaining_val).ok(),
                    used_percent: remaining_percent.map(|rem| (100.0 - rem).clamp(0.0, 100.0)),
                    remaining_percent,
                    window_duration_seconds: None,
                    resets_at: None,
                    fresh_until: Some(now + SignedDuration::seconds(30)),
                    hard_blocked: remaining_val <= 0.0,
                    applicability: Applicability::Applicable,
                    observed_at: now,
                    freshness: Freshness::Fresh,
                    provenance: Provenance {
                        source_id: source_id.clone(),
                        source_quality: SourceQuality::OfficialTelemetry,
                        observed_via: Some("claude_code_session".into()),
                    },
                });
            }
        }

        if let Some(input) = stats
            .session_input_tokens
            .and_then(|v| DecimalValue::try_new(v).ok())
        {
            if let Ok(window_id) = WindowId::new("client.claude_code.input_tokens") {
                windows.push(BudgetWindow {
                    id: window_id,
                    label: Some("Session Input Tokens".into()),
                    dimension: MetricDimension::InputTokens,
                    used: Some(input),
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
                        observed_via: Some("claude_code_session".into()),
                    },
                });
            }
        }

        if let Some(output) = stats
            .session_output_tokens
            .and_then(|v| DecimalValue::try_new(v).ok())
        {
            if let Ok(window_id) = WindowId::new("client.claude_code.output_tokens") {
                windows.push(BudgetWindow {
                    id: window_id,
                    label: Some("Session Output Tokens".into()),
                    dimension: MetricDimension::OutputTokens,
                    used: Some(output),
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
                        observed_via: Some("claude_code_session".into()),
                    },
                });
            }
        }

        if windows.is_empty() {
            warnings.push(SnapshotWarning {
                code: "claude_code_stats_missing".into(),
                message: "No Claude Code session telemetry available".into(),
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

impl Default for ClaudeCodeAdapter {
    fn default() -> Self {
        Self::new("claude")
    }
}

#[async_trait]
impl BudgetSource for ClaudeCodeAdapter {
    fn descriptor(&self) -> SourceDescriptor {
        self.descriptor.clone()
    }

    async fn probe(&self) -> Result<ProbeReport, SourceError> {
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let claude_config_exists = home
            .map(|h| h.join(".claude.json").exists() || h.join(".claude").exists())
            .unwrap_or(false);

        let output = tokio::process::Command::new(&self.command)
            .args(["--version"])
            .output()
            .await;

        if let Ok(out) = output {
            if out.status.success() {
                return Ok(ProbeReport {
                    state: ProbeState::Ready,
                });
            }
        }

        if claude_config_exists {
            Ok(ProbeReport {
                state: ProbeState::Ready,
            })
        } else {
            Ok(ProbeReport {
                state: ProbeState::BinaryMissing,
            })
        }
    }

    async fn refresh(&self) -> Result<BudgetSnapshot, SourceError> {
        Self::normalize(ClaudeCodeStats::default())
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
    fn claude_code_stats_normalizes_context_window() {
        let stats = ClaudeCodeStats {
            session_input_tokens: Some(15_000.0),
            session_output_tokens: Some(3_500.0),
            context_tokens: Some(80_000.0),
            context_limit: Some(200_000.0),
        };

        let snapshot = ClaudeCodeAdapter::normalize(stats).expect("normalize");
        assert_eq!(snapshot.windows.len(), 3);

        let ctx_window = &snapshot.windows[0];
        assert_eq!(ctx_window.dimension, MetricDimension::ContextTokens);
        assert_eq!(ctx_window.used.map(|v| v.0), Some(80_000.0));
        assert_eq!(ctx_window.limit.map(|v| v.0), Some(200_000.0));
        assert_eq!(ctx_window.remaining_percent, Some(60.0));
    }
}
