#![forbid(unsafe_code)]

pub mod error;
pub mod types;
pub mod validate;

mod tools;

use std::sync::Arc;

use camino::Utf8PathBuf;
use rmcp::{
    handler::server::{wrapper::Json, wrapper::Parameters},
    service::serve_server,
    tool, tool_handler, tool_router,
};

use argyph_core::Supervisor;

#[derive(Clone)]
struct ArgyphMcp {
    supervisor: Arc<Supervisor>,
    root: Arc<Utf8PathBuf>,
}

#[tool_router]
impl ArgyphMcp {
    #[tool(
        name = "get_index_status",
        description = "Reports the readiness of all index tiers. Cheap, safe to poll."
    )]
    async fn get_index_status(
        &self,
        Parameters(_req): Parameters<tools::get_index_status::Request>,
    ) -> Json<tools::get_index_status::Response> {
        let response = tools::get_index_status::handle(&self.supervisor, &self.root).await;
        Json(response)
    }

    #[tool(
        name = "get_repo_overview",
        description = "A repo-shaped summary served from Tier 0. Useful for 'what does this codebase do?'"
    )]
    async fn get_repo_overview(
        &self,
        Parameters(req): Parameters<tools::get_repo_overview::Request>,
    ) -> Json<tools::get_repo_overview::Response> {
        let response = tools::get_repo_overview::handle(&self.supervisor, &self.root, req).await;
        Json(response)
    }

    #[tool(
        name = "search_text",
        description = "Pure ripgrep-style regex/literal search. Available immediately at Tier 0."
    )]
    async fn search_text(
        &self,
        Parameters(req): Parameters<tools::search_text::Request>,
    ) -> Json<tools::search_text::Response> {
        let response = tools::search_text::handle(&self.supervisor, &self.root, req).await;
        Json(response)
    }
}

#[tool_handler]
impl rmcp::handler::server::ServerHandler for ArgyphMcp {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        let mut info = rmcp::model::ServerInfo::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
        );
        info.server_info = rmcp::model::Implementation::new("argyph", env!("CARGO_PKG_VERSION"));
        info.instructions = Some("Argyph code indexer for AI agents".into());
        info
    }
}

pub async fn serve(
    supervisor: Arc<Supervisor>,
    root: Utf8PathBuf,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let service = ArgyphMcp {
        supervisor,
        root: Arc::new(root),
    };
    let transport = rmcp::transport::io::stdio();
    let running = serve_server(service, transport).await?;
    running.waiting().await?;
    Ok(())
}
