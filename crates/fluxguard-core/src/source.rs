use serde::{Deserialize, Serialize};

use crate::DomainError;

/// Whether a source belongs to a client, provider, or local configuration.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Client,
    Provider,
    Local,
    UserConfigured,
}

/// Evidence quality for a source observation.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceQuality {
    OfficialStructured,
    OfficialCli,
    OfficialHeaders,
    OfficialTelemetry,
    Estimated,
    Manual,
    Experimental,
}

/// Stable identity for a source adapter.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SourceId(String);

impl SourceId {
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

/// Capabilities advertised by a source adapter.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourceCapabilities {
    pub supports_snapshot: bool,
    pub supports_push_updates: bool,
    pub supports_reset_time: bool,
    pub supports_exact_remaining_percent: bool,
    pub supports_model_scope: bool,
    pub supports_cost: bool,
}

/// Metadata describing a normalized source.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourceDescriptor {
    pub id: SourceId,
    pub kind: SourceKind,
    pub display_name: String,
    pub adapter_version: String,
    pub source_quality: SourceQuality,
    pub capabilities: SourceCapabilities,
}

impl SourceDescriptor {
    pub fn validate(&self) -> Result<(), DomainError> {
        self.id.validate()
    }
}

/// Provenance retained by every normalized budget window.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Provenance {
    pub source_id: SourceId,
    pub source_quality: SourceQuality,
    pub observed_via: Option<String>,
}

impl Provenance {
    pub fn validate(&self) -> Result<(), DomainError> {
        self.source_id.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_reject_blank_values() {
        assert_eq!(SourceId::new("  "), Err(DomainError::EmptyIdentifier));
    }

    #[test]
    fn source_quality_serializes_as_stable_snake_case() {
        let encoded = serde_json::to_string(&SourceQuality::OfficialStructured).expect("serialize");
        assert_eq!(encoded, "\"official_structured\"");
    }
}
