use std::{sync::Arc, time::Duration};

use fluxguard_mcp::{AdviceRequest, FluxGuardServer, StatusRequest};
use fluxguard_runtime::SourceRegistry;
use rmcp::{handler::server::wrapper::Parameters, Json};
use tokio::sync::Mutex;

fn server() -> FluxGuardServer {
    FluxGuardServer::new(Arc::new(Mutex::new(SourceRegistry::new(
        Duration::from_secs(1),
    ))))
}

#[tokio::test(flavor = "current_thread")]
async fn compact_status_does_not_expand_empty_source_state() {
    let response: Json<fluxguard_mcp::StatusResponse> = server()
        .resource_status(Parameters(StatusRequest {
            detail: Some("compact".into()),
            sources: Vec::new(),
        }))
        .await
        .expect("status response");

    assert_eq!(response.0.pressure, "unknown");
    assert_eq!(response.0.recommended_mode, "efficiency");
    assert!(response.0.sources.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn advice_rejects_empty_operation_as_invalid_operation() {
    let result = server()
        .resource_advice(Parameters(AdviceRequest {
            operation: String::new(),
            estimated_cost: "low".into(),
            importance: "required".into(),
        }))
        .await;

    match result {
        Err(error) => assert_eq!(error, "invalid_operation"),
        Ok(_) => panic!("empty operation must be rejected"),
    }
}
