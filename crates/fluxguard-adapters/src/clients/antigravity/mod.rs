use ::time::{Duration as SignedDuration, OffsetDateTime};
use async_trait::async_trait;
use fluxguard_core::{
    Applicability, Availability, BudgetSnapshot, BudgetWindow, Freshness, MetricDimension,
    Provenance, SnapshotWarning, SourceCapabilities, SourceDescriptor, SourceId, SourceKind,
    SourceQuality, WindowId,
};
use fluxguard_runtime::{BudgetSource, ProbeReport, ProbeState, SourceError};
use serde::Deserialize;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

/// Google Antigravity client quota and telemetry adapter.
#[derive(Clone, Debug)]
pub struct AntigravityAdapter {
    command: String,
    descriptor: SourceDescriptor,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityQuotaStats {
    #[serde(default)]
    pub five_hour_used_percent: Option<f64>,
    #[serde(default)]
    pub five_hour_remaining_percent: Option<f64>,
    #[serde(default)]
    pub weekly_used_percent: Option<f64>,
    #[serde(default)]
    pub weekly_remaining_percent: Option<f64>,
    #[serde(default)]
    pub five_hour_resets_at_unix: Option<i64>,
    #[serde(default)]
    pub weekly_resets_at_unix: Option<i64>,
}

impl AntigravityAdapter {
    pub fn new(command: impl Into<String>) -> Self {
        let id = SourceId::new("client.antigravity").expect("static source id is valid");
        Self {
            command: command.into(),
            descriptor: SourceDescriptor {
                id,
                kind: SourceKind::Client,
                display_name: "Google Antigravity".into(),
                adapter_version: env!("CARGO_PKG_VERSION").into(),
                source_quality: SourceQuality::OfficialTelemetry,
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

    pub fn normalize(stats: AntigravityQuotaStats) -> Result<BudgetSnapshot, SourceError> {
        let source_id =
            SourceId::new("client.antigravity").map_err(|_| SourceError::InvalidPayload)?;
        let descriptor = SourceDescriptor {
            id: source_id.clone(),
            kind: SourceKind::Client,
            display_name: "Google Antigravity".into(),
            adapter_version: env!("CARGO_PKG_VERSION").into(),
            source_quality: SourceQuality::OfficialTelemetry,
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

        if let Some(rem_pct) = stats.five_hour_remaining_percent.or_else(|| {
            stats
                .five_hour_used_percent
                .map(|used| (100.0 - used).clamp(0.0, 100.0))
        }) {
            let resets_at = stats
                .five_hour_resets_at_unix
                .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok());

            let window_id = WindowId::new("client.antigravity.five_hour")
                .map_err(|_| SourceError::InvalidPayload)?;

            windows.push(BudgetWindow {
                id: window_id,
                label: Some("Five-Hour Quota".into()),
                dimension: MetricDimension::Requests,
                used: None,
                limit: None,
                remaining: None,
                used_percent: Some((100.0 - rem_pct).clamp(0.0, 100.0)),
                remaining_percent: Some(rem_pct),
                window_duration_seconds: Some(5 * 3600),
                resets_at,
                fresh_until: Some(now + SignedDuration::seconds(60)),
                hard_blocked: rem_pct <= 0.0,
                applicability: Applicability::Applicable,
                observed_at: now,
                freshness: Freshness::Fresh,
                provenance: Provenance {
                    source_id: source_id.clone(),
                    source_quality: SourceQuality::OfficialTelemetry,
                    observed_via: Some("antigravity_quota".into()),
                },
            });
        }

        if let Some(rem_pct) = stats.weekly_remaining_percent.or_else(|| {
            stats
                .weekly_used_percent
                .map(|used| (100.0 - used).clamp(0.0, 100.0))
        }) {
            let resets_at = stats
                .weekly_resets_at_unix
                .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok());

            let window_id = WindowId::new("client.antigravity.weekly")
                .map_err(|_| SourceError::InvalidPayload)?;

            windows.push(BudgetWindow {
                id: window_id,
                label: Some("Weekly Quota".into()),
                dimension: MetricDimension::Requests,
                used: None,
                limit: None,
                remaining: None,
                used_percent: Some((100.0 - rem_pct).clamp(0.0, 100.0)),
                remaining_percent: Some(rem_pct),
                window_duration_seconds: Some(7 * 24 * 3600),
                resets_at,
                fresh_until: Some(now + SignedDuration::seconds(60)),
                hard_blocked: rem_pct <= 0.0,
                applicability: Applicability::Applicable,
                observed_at: now,
                freshness: Freshness::Fresh,
                provenance: Provenance {
                    source_id,
                    source_quality: SourceQuality::OfficialTelemetry,
                    observed_via: Some("antigravity_quota".into()),
                },
            });
        }

        if windows.is_empty() {
            warnings.push(SnapshotWarning {
                code: "antigravity_stats_missing".into(),
                message: "No Antigravity quota information available".into(),
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

    pub async fn probe_surfaces(&self) -> Vec<&'static str> {
        let mut surfaces = Vec::new();

        if let Ok(output) = tokio::process::Command::new(&self.command)
            .args(["--version"])
            .output()
            .await
        {
            if output.status.success() {
                surfaces.push("agy-cli");
            }
        }

        if let Ok(output) = tokio::process::Command::new("gemini")
            .args(["--version"])
            .output()
            .await
        {
            if output.status.success() {
                surfaces.push("gemini-cli");
            }
        }

        if let Ok(output) = tokio::process::Command::new("antigravity")
            .args(["--version"])
            .output()
            .await
        {
            if output.status.success() {
                surfaces.push("antigravity-bin");
            }
        }

        if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
            let gemini_dir = home.join(".gemini");
            if gemini_dir.join("antigravity-cli").exists() && !surfaces.contains(&"agy-cli") {
                surfaces.push("antigravity-cli");
            }
            if gemini_dir.join("antigravity-ide").exists() || home.join(".antigravity").exists() {
                surfaces.push("antigravity-ide");
            }
            if gemini_dir.join("antigravity").exists() {
                surfaces.push("antigravity-app");
            }
            if gemini_dir.join("settings.json").exists() {
                surfaces.push("gemini-settings");
            }
        }

        if std::env::var_os("GEMINI_API_KEY").is_some() {
            surfaces.push("gemini-api-key");
        }
        if std::env::var_os("GOOGLE_API_KEY").is_some() {
            surfaces.push("google-api-key");
        }
        if std::env::var_os("GOOGLE_GENAI_API_KEY").is_some() {
            surfaces.push("google-genai-key");
        }
        if std::env::var_os("VERTEX_API_KEY").is_some()
            || std::env::var_os("GOOGLE_APPLICATION_CREDENTIALS").is_some()
        {
            surfaces.push("vertex-credentials");
        }

        surfaces
    }
}

impl Default for AntigravityAdapter {
    fn default() -> Self {
        Self::new("agy")
    }
}

#[async_trait]
impl BudgetSource for AntigravityAdapter {
    fn descriptor(&self) -> SourceDescriptor {
        self.descriptor.clone()
    }

    async fn probe(&self) -> Result<ProbeReport, SourceError> {
        let surfaces = self.probe_surfaces().await;
        if !surfaces.is_empty() {
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
    fn antigravity_stats_normalizes_five_hour_and_weekly_windows() {
        let stats = AntigravityQuotaStats {
            five_hour_used_percent: Some(25.0),
            five_hour_remaining_percent: Some(75.0),
            weekly_used_percent: Some(40.0),
            weekly_remaining_percent: Some(60.0),
            five_hour_resets_at_unix: Some(1774915200),
            weekly_resets_at_unix: Some(1775520000),
        };

        let snapshot = AntigravityAdapter::normalize(stats).expect("normalize");
        assert_eq!(snapshot.windows.len(), 2);

        let five_h = &snapshot.windows[0];
        assert_eq!(five_h.id.as_str(), "client.antigravity.five_hour");
        assert_eq!(five_h.remaining_percent, Some(75.0));

        let weekly = &snapshot.windows[1];
        assert_eq!(weekly.id.as_str(), "client.antigravity.weekly");
        assert_eq!(weekly.remaining_percent, Some(60.0));
    }
    #[test]
    fn empty_stats_stay_unknown() {
        let snapshot =
            AntigravityAdapter::normalize(AntigravityQuotaStats::default()).expect("normalize");
        assert!(snapshot.windows.is_empty());
        assert!(matches!(snapshot.availability, Availability::Unknown));
    }

    #[tokio::test]
    async fn refresh_reports_unsupported_until_a_real_surface_exists() {
        let result = AntigravityAdapter::new("agy").refresh().await;
        assert!(matches!(result, Err(SourceError::UnsupportedVersion)));
    }
}
