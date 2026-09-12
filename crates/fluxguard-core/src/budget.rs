use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, UtcOffset};

use crate::{DomainError, Provenance, SourceDescriptor};

/// A finite decimal measurement supplied by a source.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(transparent)]
pub struct DecimalValue(pub f64);

impl DecimalValue {
    pub fn try_new(value: f64) -> Result<Self, DomainError> {
        if value.is_finite() {
            Ok(Self(value))
        } else {
            Err(DomainError::NonFiniteValue)
        }
    }
}

/// The resource dimension represented by a budget window.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricDimension {
    Requests,
    Tokens,
    InputTokens,
    OutputTokens,
    Credits,
    Currency,
    Compute,
    ContextTokens,
    Concurrency,
    Time,
    Unknown(String),
}

/// Whether a limit applies to the current operation or model.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Applicability {
    Applicable,
    NotApplicable,
    Unknown,
}

/// Freshness classification for an observation.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    Fresh,
    Aging,
    Stale,
}

/// Account or service availability independent of numeric quota values.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Allowed,
    Blocked { reason: BlockReason },
    Unknown,
}

/// Why a source reported a hard block.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockReason {
    QuotaExhausted,
    PermissionDenied,
    AccountRestricted,
    UpstreamUnavailable,
    Other(String),
}

/// A single independent resource constraint.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BudgetWindow {
    pub id: WindowId,
    pub label: Option<String>,
    pub dimension: MetricDimension,

    pub used: Option<DecimalValue>,
    pub limit: Option<DecimalValue>,
    pub remaining: Option<DecimalValue>,

    pub used_percent: Option<f64>,
    pub remaining_percent: Option<f64>,

    pub window_duration_seconds: Option<u64>,
    pub resets_at: Option<OffsetDateTime>,
    pub fresh_until: Option<OffsetDateTime>,

    pub hard_blocked: bool,
    pub applicability: Applicability,

    pub observed_at: OffsetDateTime,
    pub freshness: Freshness,
    pub provenance: Provenance,
}

impl BudgetWindow {
    pub fn validate(&self) -> Result<(), DomainError> {
        validate_percent(self.used_percent)?;
        validate_percent(self.remaining_percent)?;
        validate_decimal(self.used)?;
        validate_decimal(self.limit)?;
        validate_decimal(self.remaining)?;
        validate_utc(self.observed_at)?;

        if let Some(timestamp) = self.resets_at {
            validate_utc(timestamp)?;
        }
        if let Some(timestamp) = self.fresh_until {
            validate_utc(timestamp)?;
        }

        if let (Some(used), Some(remaining)) = (self.used_percent, self.remaining_percent) {
            if (used + remaining - 100.0).abs() > 0.01 {
                return Err(DomainError::InconsistentPercentages);
            }
        }

        if let (Some(used), Some(limit), Some(remaining)) = (self.used, self.limit, self.remaining)
        {
            if (used.0 + remaining.0 - limit.0).abs() > 0.01 {
                return Err(DomainError::InconsistentAmounts);
            }
        }

        self.id.validate()?;
        self.provenance.validate()
    }
}

/// Stable identity for one normalized budget window.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct WindowId(String);

impl WindowId {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(DomainError::EmptyIdentifier);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(&self) -> Result<(), DomainError> {
        if self.0.trim().is_empty() {
            Err(DomainError::EmptyIdentifier)
        } else {
            Ok(())
        }
    }
}

/// A normalized observation from one source.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BudgetSnapshot {
    pub source: SourceDescriptor,
    pub account_scope: Option<String>,
    pub availability: Availability,
    pub windows: Vec<BudgetWindow>,
    pub observed_at: OffsetDateTime,
    pub warnings: Vec<SnapshotWarning>,
}

impl BudgetSnapshot {
    pub fn validate(&self) -> Result<(), DomainError> {
        self.source.validate()?;
        validate_utc(self.observed_at)?;
        for window in &self.windows {
            window.validate()?;
        }
        Ok(())
    }
}

/// A non-fatal issue associated with a snapshot.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SnapshotWarning {
    pub code: String,
    pub message: String,
}

fn validate_decimal(value: Option<DecimalValue>) -> Result<(), DomainError> {
    if let Some(value) = value {
        if !value.0.is_finite() {
            return Err(DomainError::NonFiniteValue);
        }
    }
    Ok(())
}

fn validate_percent(value: Option<f64>) -> Result<(), DomainError> {
    if let Some(value) = value {
        if !value.is_finite() {
            return Err(DomainError::NonFiniteValue);
        }
        if !(0.0..=100.0).contains(&value) {
            return Err(DomainError::PercentageOutOfRange);
        }
    }
    Ok(())
}

fn validate_utc(value: OffsetDateTime) -> Result<(), DomainError> {
    if value.offset() != UtcOffset::UTC {
        Err(DomainError::NonUtcTimestamp)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SourceId, SourceKind, SourceQuality};

    fn sample_window() -> BudgetWindow {
        let source = SourceId::new("client.codex").expect("valid source");
        BudgetWindow {
            id: WindowId::new("weekly").expect("valid window"),
            label: Some("Weekly quota".into()),
            dimension: MetricDimension::Requests,
            used: Some(DecimalValue(92.0)),
            limit: Some(DecimalValue(100.0)),
            remaining: Some(DecimalValue(8.0)),
            used_percent: Some(92.0),
            remaining_percent: Some(8.0),
            window_duration_seconds: Some(604_800),
            resets_at: None,
            fresh_until: None,
            hard_blocked: false,
            applicability: Applicability::Applicable,
            observed_at: OffsetDateTime::UNIX_EPOCH,
            freshness: Freshness::Fresh,
            provenance: Provenance {
                source_id: source,
                source_quality: SourceQuality::OfficialStructured,
                observed_via: Some("app_server".into()),
            },
        }
    }

    #[test]
    fn valid_window_passes_validation() {
        assert!(sample_window().validate().is_ok());
    }

    #[test]
    fn missing_values_remain_missing_after_round_trip() {
        let mut window = sample_window();
        window.used = None;
        window.limit = None;
        window.remaining = None;
        window.used_percent = None;
        window.remaining_percent = None;

        let encoded = serde_json::to_string(&window).expect("serialize");
        let decoded: BudgetWindow = serde_json::from_str(&encoded).expect("deserialize");

        assert_eq!(decoded.used, None);
        assert_eq!(decoded.limit, None);
        assert_eq!(decoded.remaining, None);
        assert_eq!(decoded.used_percent, None);
        assert_eq!(decoded.remaining_percent, None);
    }

    #[test]
    fn snapshot_round_trips_without_losing_source_identity() {
        let source_id = SourceId::new("client.codex").expect("valid source");
        let snapshot = BudgetSnapshot {
            source: SourceDescriptor {
                id: source_id.clone(),
                kind: SourceKind::Client,
                display_name: "Codex".into(),
                adapter_version: "0.1.0".into(),
                source_quality: SourceQuality::OfficialStructured,
                capabilities: Default::default(),
            },
            account_scope: None,
            availability: Availability::Allowed,
            windows: vec![sample_window()],
            observed_at: OffsetDateTime::UNIX_EPOCH,
            warnings: Vec::new(),
        };

        let encoded = serde_json::to_string(&snapshot).expect("serialize");
        let decoded: BudgetSnapshot = serde_json::from_str(&encoded).expect("deserialize");

        assert_eq!(decoded.source.id, source_id);
        assert_eq!(decoded.windows, snapshot.windows);
        assert_eq!(decoded.validate(), Ok(()));
    }
}
