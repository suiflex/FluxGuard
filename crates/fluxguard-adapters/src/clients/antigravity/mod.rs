use ::time::OffsetDateTime;
use async_trait::async_trait;
use fluxguard_core::{
    BudgetSnapshot, MetricDimension, SourceDescriptor, SourceKind, SourceQuality,
};
use fluxguard_runtime::{BudgetSource, ProbeReport, ProbeState, SourceError};
use serde::Deserialize;

use crate::support::{self, WindowSpec};

const OBSERVED_VIA: &str = "antigravity_quota";

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

fn descriptor() -> SourceDescriptor {
    support::descriptor(
        "client.antigravity",
        SourceKind::Client,
        "Google Antigravity",
        SourceQuality::OfficialTelemetry,
        support::capabilities(true, false),
    )
}

fn remaining_percent(remaining: Option<f64>, used: Option<f64>) -> Option<f64> {
    remaining.or_else(|| used.map(|used| (100.0 - used).clamp(0.0, 100.0)))
}

fn unix(value: Option<i64>) -> Option<OffsetDateTime> {
    value.and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok())
}

impl AntigravityAdapter {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            descriptor: descriptor(),
        }
    }

    pub fn normalize(stats: AntigravityQuotaStats) -> Result<BudgetSnapshot, SourceError> {
        let source = descriptor();
        let now = OffsetDateTime::now_utc();
        let mut windows = Vec::new();

        let quotas = [
            (
                "client.antigravity.five_hour",
                "Five-Hour Quota",
                5 * 3600,
                remaining_percent(
                    stats.five_hour_remaining_percent,
                    stats.five_hour_used_percent,
                ),
                unix(stats.five_hour_resets_at_unix),
            ),
            (
                "client.antigravity.weekly",
                "Weekly Quota",
                7 * 24 * 3600,
                remaining_percent(stats.weekly_remaining_percent, stats.weekly_used_percent),
                unix(stats.weekly_resets_at_unix),
            ),
        ];
        for (id, label, duration, remaining_percent, resets_at) in quotas {
            if remaining_percent.is_none() {
                continue;
            }
            support::push_window(
                &mut windows,
                &source,
                OBSERVED_VIA,
                now,
                WindowSpec {
                    remaining_percent,
                    window_duration_seconds: Some(duration),
                    resets_at,
                    ..WindowSpec::new(id, label, MetricDimension::Requests)
                },
            )?;
        }

        Ok(support::snapshot(
            source,
            now,
            windows,
            "antigravity_stats_missing",
            "No Antigravity quota information available",
        ))
    }

    pub async fn probe_surfaces(&self) -> Vec<&'static str> {
        let mut surfaces = Vec::new();

        for (command, surface) in [
            (self.command.as_str(), "agy-cli"),
            ("gemini", "gemini-cli"),
            ("antigravity", "antigravity-bin"),
        ] {
            if binary_responds(command).await {
                surfaces.push(surface);
            }
        }

        if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
            let gemini_dir = home.join(".gemini");
            let installs = [
                (vec![gemini_dir.join("antigravity-cli")], "antigravity-cli"),
                (
                    vec![
                        gemini_dir.join("antigravity-ide"),
                        home.join(".antigravity"),
                    ],
                    "antigravity-ide",
                ),
                (vec![gemini_dir.join("antigravity")], "antigravity-app"),
                (vec![gemini_dir.join("settings.json")], "gemini-settings"),
            ];
            for (paths, surface) in installs {
                // The bundled CLI is already reported when `agy` answers directly.
                let shadowed = surface == "antigravity-cli" && surfaces.contains(&"agy-cli");
                if !shadowed && paths.iter().any(|path| path.exists()) {
                    surfaces.push(surface);
                }
            }
        }

        let env_surfaces: [(&[&str], &str); 4] = [
            (&["GEMINI_API_KEY"], "gemini-api-key"),
            (&["GOOGLE_API_KEY"], "google-api-key"),
            (&["GOOGLE_GENAI_API_KEY"], "google-genai-key"),
            (
                &["VERTEX_API_KEY", "GOOGLE_APPLICATION_CREDENTIALS"],
                "vertex-credentials",
            ),
        ];
        for (vars, surface) in env_surfaces {
            if vars.iter().any(|var| std::env::var_os(var).is_some()) {
                surfaces.push(surface);
            }
        }

        surfaces
    }
}

async fn binary_responds(command: &str) -> bool {
    tokio::process::Command::new(command)
        .args(["--version"])
        .output()
        .await
        .is_ok_and(|output| output.status.success())
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antigravity_stats_normalizes_five_hour_and_weekly_windows() {
        // Shape is FluxGuard's own normalized contract, not a vendor payload.
        let snapshot = crate::support::fixture_snapshot(
            include_str!("../../../tests/fixtures/antigravity/quota_basic.json"),
            AntigravityAdapter::normalize,
        );
        assert_eq!(snapshot.windows.len(), 2);

        let five_h = &snapshot.windows[0];
        assert_eq!(five_h.id.as_str(), "client.antigravity.five_hour");
        assert_eq!(five_h.remaining_percent, Some(75.0));
        assert!(five_h.resets_at.is_some());

        let weekly = &snapshot.windows[1];
        assert_eq!(weekly.id.as_str(), "client.antigravity.weekly");
        assert_eq!(weekly.remaining_percent, Some(60.0));
    }

    detection_only_contract!(
        AntigravityAdapter,
        AntigravityAdapter::new("agy"),
        AntigravityQuotaStats
    );
}
