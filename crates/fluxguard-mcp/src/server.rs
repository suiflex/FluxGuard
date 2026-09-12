use std::sync::Arc;

use fluxguard_core::PressureConfig;
use fluxguard_runtime::SourceRegistry;
use rmcp::{
    model::{
        ListResourcesResult, PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult,
        Resource, ResourceContents, ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    transport::stdio,
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
};
use tokio::sync::Mutex;

use crate::{
    resources::{resource_content, RESOURCE_DIAGNOSTICS, RESOURCE_SOURCES, RESOURCE_STATUS},
    tools::build_tool_router,
};

#[derive(Clone)]
pub struct FluxGuardServer {
    pub(crate) tool_router: rmcp::handler::server::router::tool::ToolRouter<Self>,
    pub(crate) registry: Arc<Mutex<SourceRegistry>>,
    pub(crate) pressure_config: PressureConfig,
}

impl FluxGuardServer {
    pub fn new(registry: Arc<Mutex<SourceRegistry>>) -> Self {
        Self {
            tool_router: build_tool_router(),
            registry,
            pressure_config: PressureConfig::default(),
        }
    }

    pub fn with_pressure_config(
        registry: Arc<Mutex<SourceRegistry>>,
        pressure_config: PressureConfig,
    ) -> Self {
        Self {
            tool_router: build_tool_router(),
            registry,
            pressure_config,
        }
    }

    pub async fn serve_stdio(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let service = self.serve(stdio()).await?;
        service.waiting().await?;
        Ok(())
    }

    #[cfg(feature = "http")]
    pub async fn serve_http(
        self,
        listener: tokio::net::TcpListener,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
        use rmcp::transport::streamable_http_server::{
            StreamableHttpServerConfig, StreamableHttpService,
        };

        let service = StreamableHttpService::new(
            move || Ok(self.clone()),
            Arc::new(LocalSessionManager::default()),
            StreamableHttpServerConfig::default(),
        );
        let router = axum::Router::new().nest_service("/mcp", service);
        axum::serve(listener, router).await?;
        Ok(())
    }
}

#[rmcp::tool_handler(router = self.tool_router)]
impl ServerHandler for FluxGuardServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![
                Resource::new(RESOURCE_STATUS, "Full status"),
                Resource::new(RESOURCE_SOURCES, "Source states"),
                Resource::new(RESOURCE_DIAGNOSTICS, "Source diagnostics"),
            ],
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, McpError> {
        let content = resource_content(self, &request.uri)
            .await
            .map_err(|_| McpError::resource_not_found("resource_not_found", None))?;
        Ok(ReadResourceResult::new(vec![ResourceContents::text(content, &request.uri)]).into())
    }
}
