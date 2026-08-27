//! MCP server exposing the native zoetrope engine over stdio.

pub mod error;
pub mod render;
pub mod source;
pub mod tools;

use std::path::PathBuf;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Implementation, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};

use crate::error::ToolError;
use crate::tools::RenderDocumentParams;

/// Built via `ServerInfo::new` + field assignment rather than a struct
/// literal: several rmcp model types are `#[non_exhaustive]`, so a literal
/// would not compile from outside the crate, and the protocol version is
/// rmcp's to choose. `Implementation` is `#[non_exhaustive]` too, so it is
/// built via `Implementation::new` rather than a struct literal with
/// `..Implementation::default()`.
fn server_info(capabilities: ServerCapabilities) -> ServerInfo {
    let mut info = ServerInfo::new(capabilities);
    info.server_info = Implementation::new("zoetrope-mcp", env!("CARGO_PKG_VERSION"));
    info.instructions = Some(
        "Renders zoetrope scene documents to MP4. Deterministic: the same \
         document always produces the same bytes. Requires ffmpeg on PATH."
            .into(),
    );
    info
}

#[derive(Clone)]
pub struct ZoetropeServer {
    tool_router: ToolRouter<Self>,
}

impl Default for ZoetropeServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router(router = tool_router)]
impl ZoetropeServer {
    pub fn new() -> Self {
        ZoetropeServer {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "render_document",
        description = "Render a zoetrope scene document to an MP4. Rendering is \
                       deterministic: the same document always produces the same \
                       bytes. Returns the output path, metadata, and sampled \
                       frames as images so you can check the result. Requires \
                       ffmpeg on PATH."
    )]
    pub async fn render_document(
        &self,
        Parameters(params): Parameters<RenderDocumentParams>,
    ) -> CallToolResult {
        match Self::render_document_impl(params) {
            Ok(result) => result,
            Err(e) => e.into_result(),
        }
    }

    fn render_document_impl(params: RenderDocumentParams) -> Result<CallToolResult, ToolError> {
        crate::source::check_fps(params.fps)?;

        let (doc, default_base) = crate::source::load_document(
            params.document.as_deref(),
            params.document_path.as_deref(),
        )?;
        let base = params
            .asset_base_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or(default_base);

        let assets = crate::source::resolve_assets(&doc, &base)?;
        let mut engine = zoetrope_core::Engine::new(doc, assets)?;

        if params.validate_only {
            let outcome = crate::render::describe(&engine, params.fps);
            return Ok(crate::tools::success(&outcome, Vec::new()));
        }

        let out = params.out.ok_or_else(|| {
            ToolError::Invalid("`out` is required unless `validateOnly` is true".into())
        })?;

        let outcome = crate::render::render_to_mp4(&mut engine, params.fps, &out)?;
        let previews =
            crate::render::sample_frames(&mut engine, params.fps, params.preview_frames)?;
        Ok(crate::tools::success(&outcome, previews))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ZoetropeServer {
    fn get_info(&self) -> ServerInfo {
        server_info(ServerCapabilities::builder().enable_tools().build())
    }
}
