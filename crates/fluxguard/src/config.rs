use std::{
    fs,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use fluxguard_core::{
    Applicability, Availability, BlockReason, BudgetSnapshot, BudgetWindow, DecimalValue,
    Freshness, MetricDimension, PressureConfig, Provenance, SourceCapabilities, SourceDescriptor,
    SourceId, SourceKind, SourceQuality, WindowId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::{Duration, OffsetDateTime};

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not read configuration")]
    Read(#[source] std::io::Error),
    #[error("invalid TOML configuration")]
    Parse(#[source] toml::de::Error),
    #[error("invalid pressure thresholds; expected 100 >= guarded >= conserve >= critical >= emergency >= 0")]
    InvalidThresholds,
    #[error("refresh interval must be greater than zero")]
    InvalidRefreshInterval,
    #[error("invalid environment override {name}: {reason}")]
    InvalidEnvironment { name: String, reason: String },
    #[error("invalid manual source {id}: {reason}")]
    InvalidManualSource { id: String, reason: String },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub pressure: PressureSettings,
    pub clients: ClientsSettings,
    pub providers: ProvidersSettings,
    pub sources: SourcesSettings,
}

impl Config {
    pub fn path() -> PathBuf {
        ProjectDirs::from("", "FluxGuard", "FluxGuard")
            .map(|dirs| dirs.config_dir().join("config.toml"))
            .unwrap_or_else(|| PathBuf::from("config.toml"))
    }

    pub fn load(path: Option<&Path>) -> Result<Self, ConfigError> {
        let path = path.map(PathBuf::from).unwrap_or_else(Self::path);
        let mut config = if path.exists() {
            let content = fs::read_to_string(path).map_err(ConfigError::Read)?;
            toml::from_str(&content).map_err(ConfigError::Parse)?
        } else {
            Self::default()
        };
        config.apply_env_overrides()?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        self.pressure
            .to_core()
            .validate()
            .map_err(|_| ConfigError::InvalidThresholds)?;
        if self.clients.codex.refresh_interval_seconds == 0 {
            return Err(ConfigError::InvalidRefreshInterval);
        }
        for source in &self.sources.manual {
            let _ = self.manual_snapshot(source)?;
        }
        Ok(())
    }

    pub fn pressure_config(&self) -> Result<PressureConfig, ConfigError> {
        let pressure = self.pressure.to_core();
        pressure
            .validate()
            .map_err(|_| ConfigError::InvalidThresholds)?;
        Ok(pressure)
    }

    pub fn manual_snapshots(&self) -> Result<Vec<BudgetSnapshot>, ConfigError> {
        self.sources
            .manual
            .iter()
            .map(|source| self.manual_snapshot(source))
            .collect()
    }

    fn manual_snapshot(
        &self,
        source: &ManualSourceSettings,
    ) -> Result<BudgetSnapshot, ConfigError> {
        let source_id =
            SourceId::new(source.id.clone()).map_err(|_| ConfigError::InvalidManualSource {
                id: source.id.clone(),
                reason: "id must not be empty".into(),
            })?;
        let dimension =
            parse_dimension(&source.dimension).ok_or_else(|| ConfigError::InvalidManualSource {
                id: source.id.clone(),
                reason: "dimension is not supported".into(),
            })?;
        let used = decimal(source.used, &source.id, "used")?;
        let limit = decimal(source.limit, &source.id, "limit")?;
        let remaining = decimal(source.remaining, &source.id, "remaining")?;
        let remaining_percent = source
            .remaining_percent
            .or_else(|| Some(remaining?.0 / limit?.0 * 100.0));
        let remaining_percent = remaining_percent.filter(|value| value.is_finite());
        let resets_at = source
            .resets_at_unix
            .map(|value| {
                OffsetDateTime::from_unix_timestamp(value).map_err(|_| {
                    ConfigError::InvalidManualSource {
                        id: source.id.clone(),
                        reason: "resets_at_unix must be a valid Unix timestamp".into(),
                    }
                })
            })
            .transpose()?;
        let now = OffsetDateTime::now_utc();
        let descriptor = SourceDescriptor {
            id: source_id.clone(),
            kind: SourceKind::UserConfigured,
            display_name: source.label.clone().unwrap_or_else(|| source.id.clone()),
            adapter_version: env!("CARGO_PKG_VERSION").into(),
            source_quality: SourceQuality::Manual,
            capabilities: SourceCapabilities {
                supports_snapshot: true,
                supports_push_updates: false,
                supports_reset_time: resets_at.is_some(),
                supports_exact_remaining_percent: source.remaining_percent.is_some(),
                supports_model_scope: false,
                supports_cost: dimension == MetricDimension::Currency,
            },
        };
        let availability = if source.hard_blocked {
            Availability::Blocked {
                reason: BlockReason::Other("manual_block".into()),
            }
        } else {
            Availability::Allowed
        };
        let window_id = WindowId::new(format!("{}.budget", source.id)).map_err(|_| {
            ConfigError::InvalidManualSource {
                id: source.id.clone(),
                reason: "id must not be empty".into(),
            }
        })?;
        let snapshot = BudgetSnapshot {
            source: descriptor,
            account_scope: None,
            availability,
            windows: vec![BudgetWindow {
                id: window_id,
                label: source.label.clone(),
                dimension,
                used,
                limit,
                remaining,
                used_percent: source.used_percent,
                remaining_percent,
                window_duration_seconds: None,
                resets_at,
                fresh_until: Some(now + Duration::seconds(30)),
                hard_blocked: source.hard_blocked,
                applicability: Applicability::Applicable,
                observed_at: now,
                freshness: Freshness::Fresh,
                provenance: Provenance {
                    source_id,
                    source_quality: SourceQuality::Manual,
                    observed_via: Some("config".into()),
                },
            }],
            observed_at: now,
            warnings: Vec::new(),
        };
        snapshot
            .validate()
            .map_err(|_| ConfigError::InvalidManualSource {
                id: source.id.clone(),
                reason: "values are inconsistent or outside supported bounds".into(),
            })?;
        Ok(snapshot)
    }

    fn apply_env_overrides(&mut self) -> Result<(), ConfigError> {
        let percents = [
            (
                "FLUXGUARD_PRESSURE_GUARDED_REMAINING_PERCENT",
                &mut self.pressure.guarded_remaining_percent,
            ),
            (
                "FLUXGUARD_PRESSURE_CONSERVE_REMAINING_PERCENT",
                &mut self.pressure.conserve_remaining_percent,
            ),
            (
                "FLUXGUARD_PRESSURE_CRITICAL_REMAINING_PERCENT",
                &mut self.pressure.critical_remaining_percent,
            ),
            (
                "FLUXGUARD_PRESSURE_EMERGENCY_REMAINING_PERCENT",
                &mut self.pressure.emergency_remaining_percent,
            ),
        ];
        for (name, target) in percents {
            if let Some(value) = read_env(name) {
                *target = parse_env_f64(name, value)?;
            }
        }

        let flags = [
            (
                "FLUXGUARD_CLIENTS_CODEX_ENABLED",
                &mut self.clients.codex.enabled,
            ),
            (
                "FLUXGUARD_CLIENTS_OPENCODE_ENABLED",
                &mut self.clients.opencode.enabled,
            ),
            (
                "FLUXGUARD_CLIENTS_COPILOT_ENABLED",
                &mut self.clients.copilot.enabled,
            ),
            (
                "FLUXGUARD_CLIENTS_CURSOR_ENABLED",
                &mut self.clients.cursor.enabled,
            ),
            (
                "FLUXGUARD_CLIENTS_CLAUDE_CODE_ENABLED",
                &mut self.clients.claude_code.enabled,
            ),
            (
                "FLUXGUARD_CLIENTS_ANTIGRAVITY_ENABLED",
                &mut self.clients.antigravity.enabled,
            ),
            (
                "FLUXGUARD_PROVIDERS_OPENAI_ENABLED",
                &mut self.providers.openai.enabled,
            ),
            (
                "FLUXGUARD_PROVIDERS_ANTHROPIC_ENABLED",
                &mut self.providers.anthropic.enabled,
            ),
            (
                "FLUXGUARD_PROVIDERS_XAI_ENABLED",
                &mut self.providers.xai.enabled,
            ),
            (
                "FLUXGUARD_PROVIDERS_ZAI_ENABLED",
                &mut self.providers.zai.enabled,
            ),
        ];
        for (name, target) in flags {
            if let Some(value) = read_env(name) {
                *target = value
                    .parse()
                    .map_err(|_| invalid_env(name, "expected true or false"))?;
            }
        }

        let commands = [
            (
                "FLUXGUARD_CLIENTS_CODEX_COMMAND",
                &mut self.clients.codex.command,
            ),
            (
                "FLUXGUARD_CLIENTS_OPENCODE_COMMAND",
                &mut self.clients.opencode.command,
            ),
            (
                "FLUXGUARD_CLIENTS_COPILOT_COMMAND",
                &mut self.clients.copilot.command,
            ),
            (
                "FLUXGUARD_CLIENTS_CLAUDE_CODE_COMMAND",
                &mut self.clients.claude_code.command,
            ),
            (
                "FLUXGUARD_CLIENTS_ANTIGRAVITY_COMMAND",
                &mut self.clients.antigravity.command,
            ),
        ];
        for (name, target) in commands {
            if let Some(value) = read_env(name) {
                *target = value;
            }
        }

        if let Some(value) = read_env("FLUXGUARD_CLIENTS_CODEX_REFRESH_INTERVAL_SECONDS") {
            self.clients.codex.refresh_interval_seconds = value.parse().map_err(|_| {
                invalid_env(
                    "FLUXGUARD_CLIENTS_CODEX_REFRESH_INTERVAL_SECONDS",
                    "expected a positive integer",
                )
            })?;
        }
        Ok(())
    }
}

fn invalid_env(name: &str, reason: &str) -> ConfigError {
    ConfigError::InvalidEnvironment {
        name: name.into(),
        reason: reason.into(),
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct PressureSettings {
    pub guarded_remaining_percent: f64,
    pub conserve_remaining_percent: f64,
    pub critical_remaining_percent: f64,
    pub emergency_remaining_percent: f64,
}

impl Default for PressureSettings {
    fn default() -> Self {
        let defaults = PressureConfig::default();
        Self {
            guarded_remaining_percent: defaults.guarded_remaining_percent,
            conserve_remaining_percent: defaults.conserve_remaining_percent,
            critical_remaining_percent: defaults.critical_remaining_percent,
            emergency_remaining_percent: defaults.emergency_remaining_percent,
        }
    }
}

impl PressureSettings {
    fn to_core(&self) -> PressureConfig {
        PressureConfig {
            guarded_remaining_percent: self.guarded_remaining_percent,
            conserve_remaining_percent: self.conserve_remaining_percent,
            critical_remaining_percent: self.critical_remaining_percent,
            emergency_remaining_percent: self.emergency_remaining_percent,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ClientsSettings {
    pub codex: CodexSettings,
    pub opencode: OpenCodeSettings,
    pub copilot: CopilotSettings,
    pub cursor: CursorSettings,
    pub claude_code: ClaudeCodeSettings,
    pub antigravity: AntigravitySettings,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct CodexSettings {
    pub enabled: bool,
    pub command: String,
    pub refresh_interval_seconds: u64,
}

impl Default for CodexSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            command: "codex".into(),
            refresh_interval_seconds: 60,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct OpenCodeSettings {
    pub enabled: bool,
    pub command: String,
}

impl Default for OpenCodeSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            command: "opencode".into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct CopilotSettings {
    pub enabled: bool,
    pub command: String,
}

impl Default for CopilotSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            command: "copilot".into(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct CursorSettings {
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct ClaudeCodeSettings {
    pub enabled: bool,
    pub command: String,
}

impl Default for ClaudeCodeSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            command: "claude".into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct AntigravitySettings {
    pub enabled: bool,
    pub command: String,
}

impl Default for AntigravitySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            command: "agy".into(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ProvidersSettings {
    pub openai: ProviderSettings,
    pub anthropic: ProviderSettings,
    pub xai: ProviderSettings,
    pub zai: ProviderSettings,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ProviderSettings {
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct SourcesSettings {
    pub manual: Vec<ManualSourceSettings>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ManualSourceSettings {
    pub id: String,
    pub dimension: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub used: Option<f64>,
    #[serde(default)]
    pub limit: Option<f64>,
    #[serde(default)]
    pub remaining: Option<f64>,
    #[serde(default)]
    pub used_percent: Option<f64>,
    #[serde(default)]
    pub remaining_percent: Option<f64>,
    #[serde(default)]
    pub resets_at_unix: Option<i64>,
    #[serde(default)]
    pub hard_blocked: bool,
}

fn decimal(value: Option<f64>, id: &str, field: &str) -> Result<Option<DecimalValue>, ConfigError> {
    value
        .map(|value| {
            DecimalValue::try_new(value).map_err(|_| ConfigError::InvalidManualSource {
                id: id.into(),
                reason: format!("{field} must be finite"),
            })
        })
        .transpose()
}

fn parse_dimension(value: &str) -> Option<MetricDimension> {
    Some(match value {
        "requests" => MetricDimension::Requests,
        "tokens" => MetricDimension::Tokens,
        "input_tokens" => MetricDimension::InputTokens,
        "output_tokens" => MetricDimension::OutputTokens,
        "credits" => MetricDimension::Credits,
        "currency" => MetricDimension::Currency,
        "compute" => MetricDimension::Compute,
        "context_tokens" => MetricDimension::ContextTokens,
        "concurrency" => MetricDimension::Concurrency,
        "time" => MetricDimension::Time,
        other if !other.trim().is_empty() => MetricDimension::Unknown(other.into()),
        _ => return None,
    })
}

fn read_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn parse_env_f64(name: &str, value: String) -> Result<f64, ConfigError> {
    value.parse().map_err(|_| ConfigError::InvalidEnvironment {
        name: name.into(),
        reason: "expected a finite number".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_threshold_order_is_rejected() {
        let mut config = Config::default();
        config.pressure.critical_remaining_percent = 60.0;
        assert!(matches!(
            config.validate(),
            Err(ConfigError::InvalidThresholds)
        ));
    }

    #[test]
    fn zero_refresh_interval_is_rejected() {
        let mut config = Config::default();
        config.clients.codex.refresh_interval_seconds = 0;
        assert!(matches!(
            config.validate(),
            Err(ConfigError::InvalidRefreshInterval)
        ));
    }

    #[test]
    fn manual_context_source_preserves_dimension_and_scope() {
        let mut config = Config::default();
        config.sources.manual.push(ManualSourceSettings {
            id: "local.context".into(),
            dimension: "context_tokens".into(),
            label: Some("Context".into()),
            used: Some(4_000.0),
            limit: Some(10_000.0),
            remaining: Some(6_000.0),
            used_percent: Some(40.0),
            remaining_percent: Some(60.0),
            resets_at_unix: None,
            hard_blocked: false,
        });
        let snapshot = config.manual_snapshots().expect("manual source").remove(0);
        assert_eq!(
            snapshot.windows[0].dimension,
            MetricDimension::ContextTokens
        );
        assert_eq!(snapshot.source.kind, SourceKind::UserConfigured);
        assert_eq!(snapshot.source.source_quality, SourceQuality::Manual);
    }

    #[test]
    fn config_deserializes_clients_and_providers_with_defaults() {
        let toml_str = r#"
            [clients.copilot]
            enabled = true
            command = "copilot"

            [providers.openai]
            enabled = true
        "#;
        let config: Config = toml::from_str(toml_str).expect("parse config");
        assert!(config.clients.copilot.enabled);
        assert_eq!(config.clients.copilot.command, "copilot");
        assert!(!config.clients.cursor.enabled);
        assert!(config.providers.openai.enabled);
        assert!(!config.providers.anthropic.enabled);
    }
}
