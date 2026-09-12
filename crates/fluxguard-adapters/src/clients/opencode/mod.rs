use std::{process::Stdio, time::Duration};

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
use tokio::{io::AsyncReadExt, process::Command};
use tokio_util::sync::CancellationToken;

const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct OpenCodeAdapter {
    command: String,
    descriptor: SourceDescriptor,
    timeout: Duration,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeStats {
    #[serde(default)]
    pub input_tokens: Option<f64>,
    #[serde(default)]
    pub output_tokens: Option<f64>,
    #[serde(default)]
    pub total_tokens: Option<f64>,
    #[serde(default)]
    pub cost: Option<f64>,
    #[serde(default)]
    pub tokens: Option<OpenCodeTokenStats>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeTokenStats {
    #[serde(default)]
    pub input: Option<f64>,
    #[serde(default)]
    pub output: Option<f64>,
    #[serde(default)]
    pub total: Option<f64>,
}

impl OpenCodeAdapter {
    pub fn new(command: impl Into<String>, timeout: Duration) -> Self {
        let id = SourceId::new("client.opencode").expect("static source id is valid");
        Self {
            command: command.into(),
            descriptor: SourceDescriptor {
                id,
                kind: SourceKind::Client,
                display_name: "OpenCode".into(),
                adapter_version: env!("CARGO_PKG_VERSION").into(),
                source_quality: SourceQuality::OfficialCli,
                capabilities: SourceCapabilities {
                    supports_snapshot: true,
                    supports_push_updates: false,
                    supports_reset_time: false,
                    supports_exact_remaining_percent: false,
                    supports_model_scope: true,
                    supports_cost: true,
                },
            },
            timeout,
        }
    }

    pub fn with_default_timeout(command: impl Into<String>) -> Self {
        Self::new(command, Duration::from_secs(10))
    }

    pub fn normalize(stats: OpenCodeStats) -> Result<BudgetSnapshot, SourceError> {
        let source_id =
            SourceId::new("client.opencode").map_err(|_| SourceError::InvalidPayload)?;
        let descriptor = SourceDescriptor {
            id: source_id.clone(),
            kind: SourceKind::Client,
            display_name: "OpenCode".into(),
            adapter_version: env!("CARGO_PKG_VERSION").into(),
            source_quality: SourceQuality::OfficialCli,
            capabilities: SourceCapabilities {
                supports_snapshot: true,
                supports_push_updates: false,
                supports_reset_time: false,
                supports_exact_remaining_percent: false,
                supports_model_scope: true,
                supports_cost: true,
            },
        };
        let now = OffsetDateTime::now_utc();
        let mut windows = Vec::new();
        let mut warnings = Vec::new();
        let input = stats.input_tokens.or_else(|| stats.tokens.as_ref()?.input);
        let output = stats
            .output_tokens
            .or_else(|| stats.tokens.as_ref()?.output);
        let total = stats.total_tokens.or_else(|| stats.tokens.as_ref()?.total);
        append_used_window(
            &mut windows,
            &mut warnings,
            &source_id,
            "input_tokens",
            MetricDimension::InputTokens,
            input,
            now,
        );
        append_used_window(
            &mut windows,
            &mut warnings,
            &source_id,
            "output_tokens",
            MetricDimension::OutputTokens,
            output,
            now,
        );
        append_used_window(
            &mut windows,
            &mut warnings,
            &source_id,
            "total_tokens",
            MetricDimension::Tokens,
            total,
            now,
        );
        append_used_window(
            &mut windows,
            &mut warnings,
            &source_id,
            "cost",
            MetricDimension::Currency,
            stats.cost,
            now,
        );
        if windows.is_empty() {
            warnings.push(SnapshotWarning {
                code: "stats_values_missing".into(),
                message: "OpenCode returned no supported usage counters".into(),
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

    async fn read_stats(&self) -> Result<OpenCodeStats, SourceError> {
        let mut child = Command::new(&self.command)
            .args(["stats", "--json", "--pure"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| SourceError::Unavailable)?;
        let stdout = child.stdout.take().ok_or(SourceError::Unavailable)?;
        let mut bytes = Vec::new();
        let read = tokio::time::timeout(
            self.timeout,
            stdout
                .take((MAX_OUTPUT_BYTES + 1) as u64)
                .read_to_end(&mut bytes),
        )
        .await
        .map_err(|_| SourceError::Timeout)?
        .map_err(|_| SourceError::Unavailable)?;
        if read > MAX_OUTPUT_BYTES {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(SourceError::InvalidPayload);
        }
        let status = child.wait().await.map_err(|_| SourceError::ProcessExited)?;
        if !status.success() {
            return Err(SourceError::Unavailable);
        }
        serde_json::from_slice(&bytes).map_err(|_| SourceError::InvalidPayload)
    }
}

#[async_trait]
impl BudgetSource for OpenCodeAdapter {
    fn descriptor(&self) -> SourceDescriptor {
        self.descriptor.clone()
    }

    async fn probe(&self) -> Result<ProbeReport, SourceError> {
        let output = Command::new(&self.command)
            .args(["stats", "--help"])
            .output()
            .await
            .map_err(|_| SourceError::Unavailable)?;
        let help = String::from_utf8_lossy(&output.stdout);
        if output.status.success() && help.contains("--json") {
            Ok(ProbeReport {
                state: ProbeState::Ready,
            })
        } else {
            Ok(ProbeReport {
                state: ProbeState::UnsupportedVersion,
            })
        }
    }

    async fn refresh(&self) -> Result<BudgetSnapshot, SourceError> {
        Self::normalize(self.read_stats().await?)
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

fn append_used_window(
    windows: &mut Vec<BudgetWindow>,
    warnings: &mut Vec<SnapshotWarning>,
    source_id: &SourceId,
    id: &str,
    dimension: MetricDimension,
    value: Option<f64>,
    now: OffsetDateTime,
) {
    let Some(value) = value else {
        return;
    };
    let Ok(value) = DecimalValue::try_new(value) else {
        warnings.push(SnapshotWarning {
            code: "invalid_stats_value".into(),
            message: id.into(),
        });
        return;
    };
    let Ok(window_id) = WindowId::new(id) else {
        return;
    };
    windows.push(BudgetWindow {
        id: window_id,
        label: Some(id.replace('_', " ")),
        dimension,
        used: Some(value),
        limit: None,
        remaining: None,
        used_percent: None,
        remaining_percent: None,
        window_duration_seconds: None,
        resets_at: None,
        fresh_until: Some(now + SignedDuration::seconds(30)),
        hard_blocked: false,
        applicability: Applicability::Applicable,
        observed_at: now,
        freshness: Freshness::Fresh,
        provenance: Provenance {
            source_id: source_id.clone(),
            source_quality: SourceQuality::OfficialCli,
            observed_via: Some("opencode_stats_json".into()),
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_stats_json_normalizes_local_usage_without_claiming_quota() {
        let payload = r#"{
            "inputTokens": 1200,
            "outputTokens": 300,
            "cost": 0.12
        }"#;
        let stats: OpenCodeStats = serde_json::from_str(payload).expect("fixture");
        let snapshot = OpenCodeAdapter::normalize(stats).expect("normalize");

        assert_eq!(snapshot.windows.len(), 3);
        assert!(snapshot
            .windows
            .iter()
            .all(|window| window.remaining_percent.is_none()));
        assert_eq!(snapshot.source.source_quality, SourceQuality::OfficialCli);
    }
}
