use std::{collections::HashMap, sync::Arc, time::Duration};

use fluxguard_core::{BudgetSnapshot, SourceId};
use thiserror::Error;
use tokio::{sync::watch, task::JoinHandle};
use tokio_util::sync::CancellationToken;

use crate::{
    diagnostics::SourceDiagnostic,
    scheduler::RefreshScheduler,
    source::{BudgetSource, SourceState},
};

/// Registry failures that are safe to return to callers.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("source is already registered")]
    DuplicateSource,
    #[error("source is not registered")]
    SourceNotFound,
}

/// Owns source adapters and publishes their latest state.
pub struct SourceRegistry {
    sources: HashMap<SourceId, Arc<dyn BudgetSource>>,
    senders: HashMap<SourceId, watch::Sender<SourceState>>,
    receivers: HashMap<SourceId, watch::Receiver<SourceState>>,
    tasks: Vec<JoinHandle<()>>,
    cancel: CancellationToken,
    scheduler: RefreshScheduler,
    started: bool,
}

impl SourceRegistry {
    pub fn new(refresh_timeout: Duration) -> Self {
        Self {
            sources: HashMap::new(),
            senders: HashMap::new(),
            receivers: HashMap::new(),
            tasks: Vec::new(),
            cancel: CancellationToken::new(),
            scheduler: RefreshScheduler::new(refresh_timeout),
            started: false,
        }
    }

    pub fn register(&mut self, source: Arc<dyn BudgetSource>) -> Result<(), RegistryError> {
        let descriptor = source.descriptor();
        let id = descriptor.id.clone();
        if self.sources.contains_key(&id) {
            return Err(RegistryError::DuplicateSource);
        }
        let (sender, receiver) = watch::channel(SourceState::initial(descriptor));
        self.sources.insert(id.clone(), source);
        self.senders.insert(id.clone(), sender);
        self.receivers.insert(id, receiver);
        Ok(())
    }

    pub fn subscribe(
        &self,
        source: &SourceId,
    ) -> Result<watch::Receiver<SourceState>, RegistryError> {
        self.receivers
            .get(source)
            .cloned()
            .ok_or(RegistryError::SourceNotFound)
    }

    pub fn start(&mut self) {
        if self.started {
            return;
        }
        self.started = true;

        for (id, source) in &self.sources {
            let Some(sender) = self.senders.get(id) else {
                continue;
            };
            let updates = sender.clone();
            let failure_updates = updates.clone();
            let cancel = self.cancel.child_token();
            let source = Arc::clone(source);
            self.tasks.push(tokio::spawn(async move {
                if let Err(error) = source.run(updates, cancel).await {
                    let current = failure_updates.borrow().clone();
                    let _ = failure_updates.send(SourceState::failed(&current, error));
                }
            }));
        }
    }

    pub async fn refresh(&self, source: &SourceId) -> Result<BudgetSnapshot, crate::SourceError> {
        tracing::debug!(source_id = %source.as_str(), "source refresh started");
        let adapter = self
            .sources
            .get(source)
            .ok_or(crate::SourceError::Unavailable)?;
        let result = self.scheduler.refresh(adapter.as_ref()).await;
        tracing::debug!(
            source_id = %source.as_str(),
            result = if result.is_ok() { "success" } else { "failure" },
            "source refresh completed"
        );
        match &result {
            Ok(snapshot) => {
                if let Some(sender) = self.senders.get(source) {
                    let _ = sender.send(SourceState::ready(snapshot.clone()));
                }
            }
            Err(error) => {
                if let Some(sender) = self.senders.get(source) {
                    let current = sender.borrow().clone();
                    let _ = sender.send(SourceState::failed(&current, error.clone()));
                }
            }
        }
        result
    }

    pub fn diagnostics(&self) -> Vec<SourceDiagnostic> {
        self.receivers
            .values()
            .map(|receiver| SourceDiagnostic::from(&*receiver.borrow()))
            .collect()
    }

    pub fn states(&self) -> Vec<SourceState> {
        self.receivers
            .values()
            .map(|receiver| receiver.borrow().clone())
            .collect()
    }

    pub async fn shutdown(&mut self) {
        self.cancel.cancel();
        for task in self.tasks.drain(..) {
            let _ = task.await;
        }
        for sender in self.senders.values() {
            let current = sender.borrow().clone();
            let _ = sender.send(SourceState::shutdown(&current));
        }
        self.started = false;
    }
}

impl Drop for SourceRegistry {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    use async_trait::async_trait;
    use fluxguard_core::{
        Applicability, Availability, BudgetSnapshot, BudgetWindow, Freshness, MetricDimension,
        Provenance, SourceCapabilities, SourceDescriptor, SourceId, SourceKind, SourceQuality,
        WindowId,
    };
    use time::OffsetDateTime;
    use tokio::{sync::watch, time::Duration};
    use tokio_util::sync::CancellationToken;

    use crate::{BudgetSource, SourceRegistry, SourceState, SourceStateKind};

    struct TestSource {
        descriptor: SourceDescriptor,
        snapshot: BudgetSnapshot,
        fail: Arc<AtomicBool>,
        fail_run: bool,
    }

    #[async_trait]
    impl BudgetSource for TestSource {
        fn descriptor(&self) -> SourceDescriptor {
            self.descriptor.clone()
        }

        async fn probe(&self) -> Result<crate::ProbeReport, crate::SourceError> {
            Ok(crate::ProbeReport {
                state: crate::ProbeState::Ready,
            })
        }

        async fn refresh(&self) -> Result<BudgetSnapshot, crate::SourceError> {
            if self.fail.load(Ordering::Relaxed) {
                Err(crate::SourceError::Unavailable)
            } else {
                Ok(self.snapshot.clone())
            }
        }

        async fn run(
            &self,
            updates: watch::Sender<SourceState>,
            cancel: CancellationToken,
        ) -> Result<(), crate::SourceError> {
            if self.fail_run {
                return Err(crate::SourceError::ProcessExited);
            }
            updates
                .send(SourceState::ready(self.snapshot.clone()))
                .map_err(|_| crate::SourceError::Other)?;
            cancel.cancelled().await;
            Ok(())
        }
    }

    fn test_source(id: &str, fail: Arc<AtomicBool>) -> Arc<TestSource> {
        test_source_with_run_failure(id, fail, false)
    }

    fn test_source_with_run_failure(
        id: &str,
        fail: Arc<AtomicBool>,
        fail_run: bool,
    ) -> Arc<TestSource> {
        let source_id = SourceId::new(id).expect("valid source");
        let descriptor = SourceDescriptor {
            id: source_id.clone(),
            kind: SourceKind::Client,
            display_name: id.into(),
            adapter_version: "test".into(),
            source_quality: SourceQuality::OfficialTelemetry,
            capabilities: SourceCapabilities::default(),
        };
        let snapshot = BudgetSnapshot {
            source: descriptor.clone(),
            account_scope: None,
            availability: Availability::Allowed,
            windows: vec![BudgetWindow {
                id: WindowId::new("weekly").expect("valid window"),
                label: None,
                dimension: MetricDimension::Requests,
                used: None,
                limit: None,
                remaining: None,
                used_percent: None,
                remaining_percent: Some(80.0),
                window_duration_seconds: None,
                resets_at: None,
                fresh_until: None,
                hard_blocked: false,
                applicability: Applicability::Applicable,
                observed_at: OffsetDateTime::UNIX_EPOCH,
                freshness: Freshness::Fresh,
                provenance: Provenance {
                    source_id,
                    source_quality: SourceQuality::OfficialTelemetry,
                    observed_via: None,
                },
            }],
            observed_at: OffsetDateTime::UNIX_EPOCH,
            warnings: Vec::new(),
        };
        Arc::new(TestSource {
            descriptor,
            snapshot,
            fail,
            fail_run,
        })
    }

    #[tokio::test(flavor = "current_thread")]
    async fn source_updates_are_published_asynchronously() {
        let fail = Arc::new(AtomicBool::new(false));
        let source = test_source("client.good", fail);
        let id = source.id();
        let mut registry = SourceRegistry::new(Duration::from_secs(1));
        registry.register(source).expect("register");
        let mut state = registry.subscribe(&id).expect("subscribe");

        registry.start();
        state.changed().await.expect("state update");
        assert_eq!(state.borrow().status, SourceStateKind::Ready);
        registry.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn one_failed_source_does_not_stop_another() {
        let good = test_source("client.good", Arc::new(AtomicBool::new(false)));
        let bad =
            test_source_with_run_failure("client.bad", Arc::new(AtomicBool::new(false)), true);
        let bad_id = bad.id();
        let good_id = good.id();
        let mut registry = SourceRegistry::new(Duration::from_secs(1));
        registry.register(good).expect("register good");
        registry.register(bad).expect("register bad");
        let mut good_state = registry.subscribe(&good_id).expect("subscribe good");
        let mut bad_state = registry.subscribe(&bad_id).expect("subscribe bad");

        registry.start();
        good_state.changed().await.expect("good update");
        bad_state.changed().await.expect("bad update");
        assert_eq!(good_state.borrow().status, SourceStateKind::Ready);
        assert_eq!(bad_state.borrow().status, SourceStateKind::Failed);
        registry.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn failed_refresh_exposes_stale_state_after_success() {
        let fail = Arc::new(AtomicBool::new(false));
        let source = test_source("client.toggle", Arc::clone(&fail));
        let id = source.id();
        let mut registry = SourceRegistry::new(Duration::from_secs(1));
        registry.register(source).expect("register");
        let mut state = registry.subscribe(&id).expect("subscribe");

        registry.refresh(&id).await.expect("initial refresh");
        state.changed().await.expect("ready update");
        fail.store(true, Ordering::Relaxed);
        assert_eq!(
            registry.refresh(&id).await,
            Err(crate::SourceError::Unavailable)
        );
        state.changed().await.expect("stale update");
        assert_eq!(state.borrow().status, SourceStateKind::Stale);
        assert!(state.borrow().snapshot.is_some());
        registry.shutdown().await;
    }
}
