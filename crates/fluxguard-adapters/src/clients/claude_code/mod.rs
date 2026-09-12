use ::time::OffsetDateTime;
use async_trait::async_trait;
use fluxguard_core::{
    BudgetSnapshot, MetricDimension, SourceDescriptor, SourceKind, SourceQuality,
};
use fluxguard_runtime::{BudgetSource, ProbeReport, ProbeState, SourceError};
use serde::Deserialize;

use crate::support::{self, WindowSpec};

const OBSERVED_VIA: &str = "claude_code_session";

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

fn descriptor() -> SourceDescriptor {
    support::descriptor(
        "client.claude_code",
        SourceKind::Client,
        "Claude Code",
        SourceQuality::OfficialTelemetry,
        support::capabilities(false, false),
    )
}

impl ClaudeCodeAdapter {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            descriptor: descriptor(),
        }
    }

    pub fn normalize(stats: ClaudeCodeStats) -> Result<BudgetSnapshot, SourceError> {
        let source = descriptor();
        let now = OffsetDateTime::now_utc();
        let mut windows = Vec::new();

        if let (Some(used), Some(limit)) = (stats.context_tokens, stats.context_limit) {
            support::push_window(
                &mut windows,
                &source,
                OBSERVED_VIA,
                now,
                WindowSpec {
                    used: Some(used),
                    limit: Some(limit),
                    ttl_seconds: 30,
                    ..WindowSpec::new(
                        "client.claude_code.context_window",
                        "Context Window",
                        MetricDimension::ContextTokens,
                    )
                },
            )?;
        }
        if stats.session_input_tokens.is_some() {
            support::push_window(
                &mut windows,
                &source,
                OBSERVED_VIA,
                now,
                WindowSpec {
                    used: stats.session_input_tokens,
                    ..WindowSpec::new(
                        "client.claude_code.input_tokens",
                        "Session Input Tokens",
                        MetricDimension::InputTokens,
                    )
                },
            )?;
        }
        if stats.session_output_tokens.is_some() {
            support::push_window(
                &mut windows,
                &source,
                OBSERVED_VIA,
                now,
                WindowSpec {
                    used: stats.session_output_tokens,
                    ..WindowSpec::new(
                        "client.claude_code.output_tokens",
                        "Session Output Tokens",
                        MetricDimension::OutputTokens,
                    )
                },
            )?;
        }

        Ok(support::snapshot(
            source,
            now,
            windows,
            "claude_code_stats_missing",
            "No Claude Code session telemetry available",
        ))
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
    fn claude_code_stats_normalizes_context_window() {
        // Shape is FluxGuard's own normalized contract, not a vendor payload.
        let stats: ClaudeCodeStats = serde_json::from_str(include_str!(
            "../../../tests/fixtures/claude_code/session_basic.json"
        ))
        .expect("fixture");

        let snapshot = ClaudeCodeAdapter::normalize(stats).expect("normalize");
        assert_eq!(snapshot.windows.len(), 3);

        let ctx_window = &snapshot.windows[0];
        assert_eq!(ctx_window.dimension, MetricDimension::ContextTokens);
        assert_eq!(ctx_window.used.map(|v| v.0), Some(80_000.0));
        assert_eq!(ctx_window.limit.map(|v| v.0), Some(200_000.0));
        assert_eq!(ctx_window.remaining_percent, Some(60.0));
    }

    detection_only_contract!(
        ClaudeCodeAdapter,
        ClaudeCodeAdapter::new("claude"),
        ClaudeCodeStats
    );
}
