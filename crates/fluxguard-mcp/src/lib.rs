//! FluxGuard MCP transport and tool handlers.

mod resources;
mod server;
mod tools;

pub use resources::{RESOURCE_DIAGNOSTICS, RESOURCE_SOURCES, RESOURCE_STATUS};
pub use server::FluxGuardServer;
pub use tools::{
    AdviceRequest, AdviceResponse, RefreshRequest, RefreshResponse, StatusRequest, StatusResponse,
};
