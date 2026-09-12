use serde_json::{json, Value};

use fluxguard_runtime::{SourceState, SourceStateKind};

use crate::server::FluxGuardServer;

pub const RESOURCE_STATUS: &str = "fluxguard://status/full";
pub const RESOURCE_SOURCES: &str = "fluxguard://sources";
pub const RESOURCE_DIAGNOSTICS: &str = "fluxguard://diagnostics";

pub(crate) async fn resource_content(server: &FluxGuardServer, uri: &str) -> Result<String, ()> {
    let registry = server.registry.lock().await;
    let states = registry.states();
    let value = match uri {
        RESOURCE_STATUS => json!({
            "sources": states.iter().map(full_source).collect::<Vec<_>>(),
        }),
        RESOURCE_SOURCES => Value::Array(states.iter().map(source_summary).collect()),
        RESOURCE_DIAGNOSTICS => Value::Array(states.iter().map(diagnostic).collect()),
        _ => return Err(()),
    };
    serde_json::to_string(&value).map_err(|_| ())
}

fn full_source(state: &SourceState) -> Value {
    json!({
        "source": state.descriptor,
        "state": state_name(&state.status),
        "snapshot": state.snapshot.as_ref().map(sanitized_snapshot),
        "last_error": state.last_error.as_ref().map(source_error_name),
        "updated_at": state.updated_at,
    })
}

fn source_summary(state: &SourceState) -> Value {
    json!({
        "id": state.descriptor.id,
        "kind": state.descriptor.kind,
        "quality": state.descriptor.source_quality,
        "state": state_name(&state.status),
    })
}

fn diagnostic(state: &SourceState) -> Value {
    json!({
        "source": state.descriptor.id,
        "adapter_version": state.descriptor.adapter_version,
        "state": state_name(&state.status),
        "last_refresh": state.snapshot.as_ref().map(|snapshot| snapshot.observed_at),
        "last_error_category": state.last_error.as_ref().map(source_error_name),
    })
}

fn sanitized_snapshot(snapshot: &fluxguard_core::BudgetSnapshot) -> Value {
    let mut value = serde_json::to_value(snapshot).unwrap_or_else(|_| json!({}));
    if let Some(object) = value.as_object_mut() {
        object.insert("account_scope".into(), Value::Null);
    }
    value
}

fn state_name(state: &SourceStateKind) -> &'static str {
    match state {
        SourceStateKind::Pending => "pending",
        SourceStateKind::Ready => "ready",
        SourceStateKind::Failed => "failed",
        SourceStateKind::Stale => "stale",
        SourceStateKind::Shutdown => "shutdown",
    }
}

fn source_error_name(error: &fluxguard_runtime::SourceError) -> &'static str {
    match error {
        fluxguard_runtime::SourceError::Unavailable => "source_unavailable",
        fluxguard_runtime::SourceError::Unauthenticated => "source_unauthenticated",
        fluxguard_runtime::SourceError::UnsupportedVersion => "source_unsupported",
        fluxguard_runtime::SourceError::Timeout => "source_timeout",
        fluxguard_runtime::SourceError::RateLimited => "source_rate_limited",
        fluxguard_runtime::SourceError::Protocol => "source_protocol_error",
        fluxguard_runtime::SourceError::InvalidPayload => "source_protocol_error",
        fluxguard_runtime::SourceError::ProcessExited => "source_unavailable",
        fluxguard_runtime::SourceError::PermissionDenied => "source_unavailable",
        fluxguard_runtime::SourceError::Other => "source_unavailable",
    }
}
