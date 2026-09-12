use std::sync::RwLock;

use async_trait::async_trait;
use fluxguard_core::{BudgetSnapshot, SourceDescriptor, SourceId};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::source::{BudgetSource, ProbeReport, ProbeState, SourceError, SourceState};

/// Local user-configured source for context, time, currency, or other budgets.
pub struct ManualSource {
    snapshot: RwLock<BudgetSnapshot>,
}

impl ManualSource {
    pub fn new(snapshot: BudgetSnapshot) -> Self {
        Self {
            snapshot: RwLock::new(snapshot),
        }
    }

    pub fn update(&self, snapshot: BudgetSnapshot) -> Result<(), SourceError> {
        *self.snapshot.write().map_err(|_| SourceError::Other)? = snapshot;
        Ok(())
    }
}

#[async_trait]
impl BudgetSource for ManualSource {
    fn descriptor(&self) -> SourceDescriptor {
        self.snapshot
            .read()
            .map(|snapshot| snapshot.source.clone())
            .unwrap_or_else(|_| SourceDescriptor {
                id: SourceId::new("manual.unavailable").expect("static source id is valid"),
                kind: fluxguard_core::SourceKind::UserConfigured,
                display_name: "Manual budget".into(),
                adapter_version: env!("CARGO_PKG_VERSION").into(),
                source_quality: fluxguard_core::SourceQuality::Manual,
                capabilities: Default::default(),
            })
    }

    async fn probe(&self) -> Result<ProbeReport, SourceError> {
        Ok(ProbeReport {
            state: ProbeState::Ready,
        })
    }

    async fn refresh(&self) -> Result<BudgetSnapshot, SourceError> {
        self.snapshot
            .read()
            .map(|snapshot| snapshot.clone())
            .map_err(|_| SourceError::Other)
    }

    async fn run(
        &self,
        updates: watch::Sender<SourceState>,
        cancel: CancellationToken,
    ) -> Result<(), SourceError> {
        updates
            .send(SourceState::ready(self.refresh().await?))
            .map_err(|_| SourceError::Other)?;
        cancel.cancelled().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fluxguard_core::{
        Applicability, Availability, BudgetWindow, Freshness, MetricDimension, Provenance,
        SourceCapabilities, SourceKind, SourceQuality, WindowId,
    };
    use time::OffsetDateTime;

    fn snapshot() -> BudgetSnapshot {
        let id = SourceId::new("manual.context").expect("source id");
        let source = SourceDescriptor {
            id: id.clone(),
            kind: SourceKind::UserConfigured,
            display_name: "Manual context".into(),
            adapter_version: "test".into(),
            source_quality: SourceQuality::Manual,
            capabilities: SourceCapabilities::default(),
        };
        BudgetSnapshot {
            source,
            account_scope: None,
            availability: Availability::Allowed,
            windows: vec![BudgetWindow {
                id: WindowId::new("context").expect("window id"),
                label: None,
                dimension: MetricDimension::ContextTokens,
                used: None,
                limit: None,
                remaining: None,
                used_percent: Some(40.0),
                remaining_percent: Some(60.0),
                window_duration_seconds: None,
                resets_at: None,
                fresh_until: None,
                hard_blocked: false,
                applicability: Applicability::Applicable,
                observed_at: OffsetDateTime::UNIX_EPOCH,
                freshness: Freshness::Fresh,
                provenance: Provenance {
                    source_id: id,
                    source_quality: SourceQuality::Manual,
                    observed_via: Some("config".into()),
                },
            }],
            observed_at: OffsetDateTime::UNIX_EPOCH,
            warnings: Vec::new(),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn manual_source_returns_configured_snapshot() {
        let source = ManualSource::new(snapshot());
        let current = source.refresh().await.expect("refresh");
        assert_eq!(current.windows[0].dimension, MetricDimension::ContextTokens);
    }
}
