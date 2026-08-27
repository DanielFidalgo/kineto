//! MCP server exposing the native kineto engine over stdio.

pub mod error;
pub mod render;
pub mod resources;
pub mod source;
pub mod storyboard;
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
    info.server_info = Implementation::new("kineto-mcp", env!("CARGO_PKG_VERSION"));
    // The claim is about the *frames*, not the MP4: the container embeds
    // encoder version and thread count, so two runs of ffmpeg over an
    // identical frame sequence can differ byte-for-byte.
    info.instructions = Some(
        "Renders kineto scene documents to MP4. Rendering is deterministic: \
         the same document always produces the same frames. Encoding to MP4 \
         requires ffmpeg on PATH; `validateOnly` calls do not need it."
            .into(),
    );
    info
}

#[derive(Clone)]
pub struct KinetoServer {
    tool_router: ToolRouter<Self>,
}

impl Default for KinetoServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router(router = tool_router)]
impl KinetoServer {
    pub fn new() -> Self {
        KinetoServer {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "render_document",
        description = "Render a kineto scene document to an MP4. Rendering is \
                       deterministic: the same document always produces the same \
                       frames. Returns the output path, metadata, and sampled \
                       frames as images so you can check the result. Requires \
                       ffmpeg on PATH, except for `validateOnly` calls."
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
        let (doc, default_base) = crate::source::load_document(
            params.document.as_deref(),
            params.document_path.as_deref(),
        )?;

        // Spec §4.1: explicit argument, else the document's own `defaultFps`,
        // else 30. Resolved *after* loading because the document is where the
        // middle option lives.
        let (fps, from_document) = match params.fps {
            Some(fps) => (fps, false),
            None => match doc.default_fps {
                Some(fps) => (i64::from(fps), true),
                None => (crate::tools::default_fps(), false),
            },
        };
        // `crates/core` does not validate `default_fps`, so an unusable one
        // reaches us here. Say where the number came from: the caller passed
        // no `fps` and would otherwise be told off for a value they never sent.
        crate::source::check_fps(fps).map_err(|e| {
            if from_document {
                ToolError::Invalid(format!("the document's `defaultFps` is unusable — {e}"))
            } else {
                e
            }
        })?;
        crate::source::check_canvas_size(doc.size.w, doc.size.h)?;

        let base = params
            .asset_base_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or(default_base);

        let assets = crate::source::resolve_assets(&doc, &base)?;
        let mut engine = kineto_core::Engine::new(doc, assets)?;

        if params.validate_only {
            let outcome = crate::render::describe(&engine, fps);
            return Ok(crate::tools::success(&outcome, Vec::new()));
        }

        let out = params.out.ok_or_else(|| {
            ToolError::Invalid("`out` is required unless `validateOnly` is true".into())
        })?;

        let outcome = crate::render::render_to_mp4(&mut engine, fps, &out)?;
        let previews = crate::render::sample_frames(&mut engine, fps, params.preview_frames)?;
        Ok(crate::tools::success(&outcome, previews))
    }

    #[tool(
        name = "render_asciicast",
        description = "Render an asciicast v2 terminal recording (.cast) to an \
                       MP4. Renders from the event data rather than capturing \
                       pixels, so the same recording always produces the same \
                       frames, faster than realtime. Returns the output path, \
                       metadata, and sampled frames as images. Requires ffmpeg \
                       on PATH, except for `validateOnly` calls."
    )]
    pub async fn render_asciicast(
        &self,
        Parameters(params): Parameters<crate::tools::RenderAsciicastParams>,
    ) -> CallToolResult {
        match Self::render_asciicast_impl(params) {
            Ok(result) => result,
            Err(e) => e.into_result(),
        }
    }

    fn render_asciicast_impl(
        params: crate::tools::RenderAsciicastParams,
    ) -> Result<CallToolResult, ToolError> {
        crate::source::check_fps(params.fps)?;

        let data = std::fs::read_to_string(&params.cast_path).map_err(|e| ToolError::Io {
            context: "reading asciicast",
            path: params.cast_path.clone(),
            source: e,
        })?;
        let cast = kineto_asciicast::parse_cast(&data)
            .map_err(|e| ToolError::Invalid(format!("invalid asciicast: {e}")))?;

        let (doc, assets) = kineto_asciicast::cast_to_document(&cast, &params.resolved_theme());
        crate::source::check_canvas_size(doc.size.w, doc.size.h)?;

        let mut store = kineto_core::AssetStore::new();
        for (id, bytes) in assets {
            store.add_bytes(&id, bytes.to_vec());
        }
        let mut engine = kineto_core::Engine::new(doc, store)?;

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

    #[tool(
        name = "render_storyboard",
        description = "Render an ordered list of images into an MP4, each held \
                       for a given duration with an optional caption. Use this \
                       to turn a sequence of screenshots into a watchable clip \
                       — for example, showing the steps of a browser run. \
                       Rendering is deterministic: the same frames always \
                       produce the same pixels. Requires ffmpeg on PATH, \
                       except for `validateOnly` calls."
    )]
    pub async fn render_storyboard(
        &self,
        Parameters(params): Parameters<crate::tools::RenderStoryboardParams>,
    ) -> CallToolResult {
        match Self::render_storyboard_impl(params) {
            Ok(result) => result,
            Err(e) => e.into_result(),
        }
    }

    fn render_storyboard_impl(
        params: crate::tools::RenderStoryboardParams,
    ) -> Result<CallToolResult, ToolError> {
        crate::source::check_fps(params.fps)?;

        let frames: Vec<crate::storyboard::Frame> = params
            .frames
            .iter()
            .map(|f| crate::storyboard::Frame {
                image: f.image.clone(),
                duration_ms: f.duration_ms,
                caption: f.caption.clone(),
            })
            .collect();

        let size = match (params.width, params.height) {
            (Some(w), Some(h)) => Some((w, h)),
            (None, None) => None,
            _ => {
                return Err(ToolError::Invalid(
                    "provide both `width` and `height`, or neither".into(),
                ));
            }
        };

        let doc = crate::storyboard::build(&frames, size)?;
        crate::source::check_canvas_size(doc.size.w, doc.size.h)?;

        // Storyboard image srcs are the caller's own paths — absolute, or
        // relative to the server's working directory. `resolve_assets`
        // handles both.
        let base = std::env::current_dir().map_err(|e| ToolError::Io {
            context: "reading current directory",
            path: ".".into(),
            source: e,
        })?;
        let assets = crate::source::resolve_assets(&doc, &base)?;
        let mut engine = kineto_core::Engine::new(doc, assets)?;

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
impl ServerHandler for KinetoServer {
    fn get_info(&self) -> ServerInfo {
        server_info(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
    }

    async fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListResourcesResult, rmcp::ErrorData> {
        Ok(rmcp::model::ListResourcesResult::with_all_items(
            crate::resources::list(),
        ))
    }

    async fn read_resource(
        &self,
        request: rmcp::model::ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, rmcp::ErrorData> {
        let uri = request.uri.clone();
        let text = crate::resources::read(&uri).ok_or_else(|| {
            rmcp::ErrorData::resource_not_found(format!("unknown resource: {uri}"), None)
        })?;
        Ok(rmcp::model::ReadResourceResult::new(vec![
            rmcp::model::ResourceContents::TextResourceContents {
                uri,
                mime_type: Some("application/json".into()),
                text,
                meta: None,
            },
        ])
        .into())
    }
}
