use ::time::OffsetDateTime;
use async_trait::async_trait;
use fluxguard_core::{BudgetSnapshot, SourceDescriptor, SourceId};
use thiserror::Error;
use tokio::sync::watch;
use tokio::time::{self, Duration, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_secs(60);

/// Typed failures exposed by a source adapter.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SourceError {
    #[error("source unavailable")]
    Unavailable,
    #[error("source authentication required")]
    Unauthenticated,
    #[error("source version unsupported")]
    UnsupportedVersion,
    #[error("source request timed out")]
    Timeout,
    #[error("source rate limited")]
    RateLimited,
    #[error("source protocol error")]
    Protocol,
    #[error("source payload invalid")]
    InvalidPayload,
    #[error("source process exited")]
    ProcessExited,
    #[error("source permission denied")]
    PermissionDenied,
    #[error("source operation failed")]
    Other,
}

/// Result of cheap source discovery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeState {
    Ready,
    BinaryMissing,
    NotAuthenticated,
    UnsupportedVersion,
    Disabled,
    Partial,
}

/// Discovery report without sensitive upstream details.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeReport {
    pub state: ProbeState,
}

/// Lifecycle state published through a watch channel.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceState {
    pub descriptor: SourceDescriptor,
    pub snapshot: Option<BudgetSnapshot>,
    pub status: SourceStateKind,
    pub last_error: Option<SourceError>,
    pub updated_at: OffsetDateTime,
}

impl SourceState {
    pub fn initial(descriptor: SourceDescriptor) -> Self {
        Self {
            descriptor,
            snapshot: None,
            status: SourceStateKind::Pending,
            last_error: None,
            updated_at: OffsetDateTime::now_utc(),
        }
    }

    pub fn ready(snapshot: BudgetSnapshot) -> Self {
        Self {
            descriptor: snapshot.source.clone(),
            snapshot: Some(snapshot),
            status: SourceStateKind::Ready,
            last_error: None,
            updated_at: OffsetDateTime::now_utc(),
        }
    }

    pub fn failed(previous: &Self, error: SourceError) -> Self {
        Self {
            descriptor: previous.descriptor.clone(),
            snapshot: previous.snapshot.clone(),
            status: if previous.snapshot.is_some() {
                SourceStateKind::Stale
            } else {
                SourceStateKind::Failed
            },
            last_error: Some(error),
            updated_at: OffsetDateTime::now_utc(),
        }
    }

    pub fn shutdown(previous: &Self) -> Self {
        Self {
            descriptor: previous.descriptor.clone(),
            snapshot: previous.snapshot.clone(),
            status: SourceStateKind::Shutdown,
            last_error: previous.last_error.clone(),
            updated_at: OffsetDateTime::now_utc(),
        }
    }
}

/// Published source lifecycle status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceStateKind {
    Pending,
    Ready,
    Failed,
    Stale,
    Shutdown,
}

/// Source abstraction implemented by client and provider adapters.
#[async_trait]
pub trait BudgetSource: Send + Sync {
    fn descriptor(&self) -> SourceDescriptor;

    async fn probe(&self) -> Result<ProbeReport, SourceError>;

    async fn refresh(&self) -> Result<BudgetSnapshot, SourceError>;

    async fn run(
        &self,
        updates: watch::Sender<SourceState>,
        cancel: CancellationToken,
    ) -> Result<(), SourceError> {
        let mut interval = time::interval(DEFAULT_REFRESH_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                _ = interval.tick() => {
                    match self.refresh().await {
                        Ok(snapshot) => {
                            updates.send(SourceState::ready(snapshot)).map_err(|_| SourceError::Other)?;
                        }
                        // A source that reports itself unsupported will not
                        // become supported by polling; surface it once and stop.
                        Err(SourceError::UnsupportedVersion) => {
                            return Err(SourceError::UnsupportedVersion);
                        }
                        Err(error) => {
                            let current = updates.borrow().clone();
                            updates.send(SourceState::failed(&current, error)).map_err(|_| SourceError::Other)?;
                        }
                    }
                }
            }
        }
    }

    fn id(&self) -> SourceId {
        self.descriptor().id
    }
}

/// `run` strategy for sources that read a one-shot local snapshot: refresh
/// once, publish it, then idle until cancelled.
pub async fn publish_once(
    source: &(impl BudgetSource + ?Sized),
    updates: watch::Sender<SourceState>,
    cancel: CancellationToken,
) -> Result<(), SourceError> {
    let snapshot = source.refresh().await?;
    updates
        .send(SourceState::ready(snapshot))
        .map_err(|_| SourceError::Other)?;
    cancel.cancelled().await;
    Ok(())
}
