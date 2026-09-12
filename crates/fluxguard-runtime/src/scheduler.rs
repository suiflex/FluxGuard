use std::time::Duration;

use crate::source::{BudgetSource, SourceError};
use fluxguard_core::BudgetSnapshot;
use tokio::time;

/// Bounds explicit source refresh calls.
#[derive(Clone, Debug)]
pub struct RefreshScheduler {
    timeout: Duration,
}

impl RefreshScheduler {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    pub async fn refresh(&self, source: &dyn BudgetSource) -> Result<BudgetSnapshot, SourceError> {
        time::timeout(self.timeout, source.refresh())
            .await
            .map_err(|_| SourceError::Timeout)?
    }
}
