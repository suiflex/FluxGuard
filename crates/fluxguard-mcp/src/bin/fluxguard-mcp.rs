use std::{sync::Arc, time::Duration};

use fluxguard_mcp::FluxGuardServer;
use fluxguard_runtime::SourceRegistry;
use tokio::sync::Mutex;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let registry = Arc::new(Mutex::new(SourceRegistry::new(Duration::from_secs(10))));
    FluxGuardServer::new(registry).serve_stdio().await
}
