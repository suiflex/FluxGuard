use fluxguard_core::SourceDescriptor;
use time::OffsetDateTime;

use crate::source::{SourceError, SourceState, SourceStateKind};

/// Operational state safe to expose through doctor or MCP diagnostics.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceDiagnostic {
    pub descriptor: SourceDescriptor,
    pub state: SourceStateKind,
    pub last_refresh: Option<OffsetDateTime>,
    pub last_error: Option<SourceError>,
}

impl From<&SourceState> for SourceDiagnostic {
    fn from(state: &SourceState) -> Self {
        Self {
            descriptor: state.descriptor.clone(),
            state: state.status.clone(),
            last_refresh: state.snapshot.as_ref().map(|snapshot| snapshot.observed_at),
            last_error: state.last_error.clone(),
        }
    }
}
