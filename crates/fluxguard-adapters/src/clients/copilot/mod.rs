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
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

/// GitHub Copilot quota and entitlement adapter.
#[derive(Clone, Debug)]
pub struct CopilotAdapter {
    command: String,
    descriptor: SourceDescriptor,
    timeout: Duration,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotQuotaStats {
    #[serde(default)]
    pub entitlement_requests: Option<f64>,
    #[serde(default)]
    pub used_requests: Option<f64>,
    #[serde(default)]
    pub remaining_requests: Option<f64>,
    #[serde(default)]
    pub remaining_percent: Option<f64>,
    #[serde(default)]
    pub reset_date_unix: Option<i64>,
    #[serde(default)]
    pub reset_date: Option<String>,
}

impl CopilotAdapter {
    pub fn new(command: impl Into<String>, timeout: Duration) -> Self {
        let id = SourceId::new("client.copilot").expect("static source id is valid");
        Self {
            command: command.into(),
            descriptor: SourceDescriptor {
                id,
                kind: SourceKind::Client,
                display_name: "GitHub Copilot".into(),
                adapter_version: env!("CARGO_PKG_VERSION").into(),
                source_quality: SourceQuality::OfficialStructured,
                capabilities: SourceCapabilities {
                    supports_snapshot: true,
                    supports_push_updates: false,
                    supports_reset_time: true,
                    supports_exact_remaining_percent: true,
                    supports_model_scope: true,
                    supports_cost: false,
                },
            },
            timeout,
        }
    }

    pub fn with_default_timeout(command: impl Into<String>) -> Self {
        Self::new(command, Duration::from_secs(10))
    }

    pub fn normalize(stats: CopilotQuotaStats) -> Result<BudgetSnapshot, SourceError> {
        let source_id = SourceId::new("client.copilot").map_err(|_| SourceError::InvalidPayload)?;
        let descriptor = SourceDescriptor {
            id: source_id.clone(),
            kind: SourceKind::Client,
            display_name: "GitHub Copilot".into(),
            adapter_version: env!("CARGO_PKG_VERSION").into(),
            source_quality: SourceQuality::OfficialStructured,
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
        let mut warnings = Vec::new();

        let resets_at = stats
            .reset_date_unix
            .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok())
            .or_else(|| {
                stats.reset_date.as_deref().and_then(|date_str| {
                    OffsetDateTime::parse(date_str, &time::format_description::well_known::Rfc3339)
                        .ok()
                })
            });

        let used = stats
            .used_requests
            .and_then(|val| DecimalValue::try_new(val).ok());
        let limit = stats
            .entitlement_requests
            .and_then(|val| DecimalValue::try_new(val).ok());
        let remaining = stats
            .remaining_requests
            .and_then(|val| DecimalValue::try_new(val).ok())
            .or_else(|| {
                let limit_val = limit?.0;
                let used_val = used?.0;
                DecimalValue::try_new((limit_val - used_val).max(0.0)).ok()
            });

        let remaining_percent = stats.remaining_percent.or_else(|| {
            let limit_val = limit?.0;
            let rem_val = remaining?.0;
            if limit_val > 0.0 {
                Some((rem_val / limit_val) * 100.0)
            } else {
                None
            }
        });

        let window_id =
            WindowId::new("client.copilot.quota").map_err(|_| SourceError::InvalidPayload)?;

        let windows = vec![BudgetWindow {
            id: window_id,
            label: Some("Copilot Quota".into()),
            dimension: MetricDimension::Requests,
            used,
            limit,
            remaining,
            used_percent: remaining_percent.map(|rem| (100.0 - rem).clamp(0.0, 100.0)),
            remaining_percent,
            window_duration_seconds: None,
            resets_at,
            fresh_until: Some(now + SignedDuration::seconds(60)),
            hard_blocked: remaining_percent.map(|rem| rem <= 0.0).unwrap_or(false),
            applicability: Applicability::Applicable,
            observed_at: now,
            freshness: Freshness::Fresh,
            provenance: Provenance {
                source_id,
                source_quality: SourceQuality::OfficialStructured,
                observed_via: Some("copilot_sdk_quota".into()),
            },
        }];

        if windows
            .iter()
            .all(|w| w.used.is_none() && w.remaining_percent.is_none())
        {
            warnings.push(SnapshotWarning {
                code: "copilot_stats_missing".into(),
                message: "Copilot returned no quota or usage information".into(),
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

    async fn read_stats(&self) -> Result<CopilotQuotaStats, SourceError> {
        let mut child = Command::new(&self.command)
            .args(["copilot", "quota", "--json"])
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
impl BudgetSource for CopilotAdapter {
    fn descriptor(&self) -> SourceDescriptor {
        self.descriptor.clone()
    }

    async fn probe(&self) -> Result<ProbeReport, SourceError> {
        let output = Command::new(&self.command)
            .args(["--version"])
            .output()
            .await;

        match output {
            Ok(out) if out.status.success() => Ok(ProbeReport {
                state: ProbeState::Ready,
            }),
            Ok(_) => Ok(ProbeReport {
                state: ProbeState::UnsupportedVersion,
            }),
            Err(_) => Ok(ProbeReport {
                state: ProbeState::BinaryMissing,
            }),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copilot_stats_normalizes_to_structured_quota() {
        let payload = r#"{
            "entitlementRequests": 1000,
            "usedRequests": 250,
            "remainingRequests": 750,
            "remainingPercent": 75.0,
            "resetDateUnix": 1774915200
        }"#;

        let stats: CopilotQuotaStats = serde_json::from_str(payload).expect("fixture");
        let snapshot = CopilotAdapter::normalize(stats).expect("normalize");

        assert_eq!(snapshot.windows.len(), 1);
        let window = &snapshot.windows[0];
        assert_eq!(window.dimension, MetricDimension::Requests);
        assert_eq!(window.used.map(|v| v.0), Some(250.0));
        assert_eq!(window.remaining.map(|v| v.0), Some(750.0));
        assert_eq!(window.remaining_percent, Some(75.0));
        assert_eq!(
            snapshot.source.source_quality,
            SourceQuality::OfficialStructured
        );
    }
}
