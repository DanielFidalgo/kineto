//! MCP server exposing the native kineto engine over stdio.

pub mod chart;
pub mod check;
pub mod error;
pub mod examples;
pub mod motion;
pub mod render;
pub mod resources;
pub mod session;
pub mod source;
pub mod storyboard;
pub mod timeline;
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
         requires ffmpeg on PATH; `validateOnly` calls do not need it.\n\n\
         Before authoring, read the `kineto://example/` documents. They are \
         short and exist to be imitated, and each is a different *shot type*: \
         statement, split, cards, reveal, flow, metric, steps. The \
         `kineto://corpus/` documents are renderer tests — valid, but written \
         to exercise easings and group nesting rather than to be copied.\n\n\
         What separates a video from a slide deck, in this format: one idea \
         per scene; a relationship drawn as a path between two things rather \
         than described in a sentence; a quantity shown as a bar or a line \
         rather than asserted; the number itself on screen, large. A scene of \
         centred prose is the thing to avoid, and `check_document` will say \
         so.\n\n\
         Vary the shot. A sequence reads as a slide deck when every scene has \
         the same shape — a header, a title, a paragraph, repeated. Alternate: \
         a full-bleed statement with no chrome, a split with a panel, a set of \
         cards entering in sequence, a number shown as a bar. Gradients, \
         rounded corners, shadows and clip windows exist for this; a flat \
         rectangle on a flat background is a slide.\n\n\
         The working order is cheapest-first: `check_document` for \
         correctness and pacing (no images, a fraction of the cost), \
         `preview_document` for chosen moments when you need to judge how it \
         looks, and `render_document` once, at the end."
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
                       ffmpeg on PATH, except for `validateOnly` calls. \
                       Render last, not first: run `check_document` and fix \
                       what it reports before spending a render. If you are \
                       authoring a document, read `kineto://example/flow` \
                       first — one idea per scene, relationships drawn as \
                       paths, quantities shown rather than claimed."
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

        // Resolved *after* loading because the document is where the middle
        // option lives.
        let fps = crate::source::resolve_fps(params.fps, &doc)?;
        crate::source::check_canvas_size(doc.size.w, doc.size.h)?;

        let base = params
            .asset_base_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or(default_base);

        let assets = crate::source::resolve_assets(&doc, &base)?;
        let timeline = crate::timeline::summary(&doc);
        let mut engine = kineto_core::Engine::new(doc, assets)?;

        if params.validate_only {
            let outcome = crate::render::describe(&engine, fps).with_timeline(timeline);
            return Ok(crate::tools::success(&outcome, Vec::new()));
        }

        let out = params.out.ok_or_else(|| {
            ToolError::Invalid("`out` is required unless `validateOnly` is true".into())
        })?;

        let outcome =
            crate::render::render_to_file(&mut engine, fps, &out)?.with_timeline(timeline);
        let previews = crate::render::sample_frames(&mut engine, fps, params.preview_frames)?;
        Ok(crate::tools::success(&outcome, previews))
    }

    #[tool(
        name = "preview_document",
        description = "Look at chosen moments of a kineto scene document \
                       without rendering a video. Give the times you care \
                       about in milliseconds and get those frames back as \
                       images, each labelled with the frame it resolved to. \
                       Writes no file and does not need ffmpeg, so this is the \
                       cheap way to check a document while you are still \
                       changing it — iterate here, then call `render_document` \
                       once it looks right."
    )]
    pub async fn preview_document(
        &self,
        Parameters(params): Parameters<crate::tools::PreviewDocumentParams>,
    ) -> CallToolResult {
        match Self::preview_document_impl(params) {
            Ok(result) => result,
            Err(e) => e.into_result(),
        }
    }

    fn preview_document_impl(
        params: crate::tools::PreviewDocumentParams,
    ) -> Result<CallToolResult, ToolError> {
        let (doc, default_base) = crate::source::load_document(
            params.document.as_deref(),
            params.document_path.as_deref(),
        )?;

        let fps = crate::source::resolve_fps(params.fps, &doc)?;
        crate::source::check_canvas_size(doc.size.w, doc.size.h)?;

        let base = params
            .asset_base_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or(default_base);

        let assets = crate::source::resolve_assets(&doc, &base)?;
        // Measured before `Engine::new` takes ownership of the document.
        let timeline = crate::timeline::summary(&doc);
        let mut engine = kineto_core::Engine::new(doc, assets)?;

        // Resolve before rasterizing: a bad moment costs the caller nothing.
        let (outcome, frames) = crate::render::resolve_preview(
            &engine,
            fps,
            &timeline,
            &params.at_ms,
            &params.at_scenes,
        )?;
        let previews = crate::render::encode_frames(&mut engine, fps, &frames)?;
        Ok(crate::tools::preview_success(&outcome, &frames, previews))
    }

    #[tool(
        name = "check_document",
        description = "Check a kineto scene document for defects at chosen \
                       moments, without rendering anything. Reports only what \
                       is wrong — text invisible against its background, \
                       elements animated off the canvas, text overrunning the \
                       edge, fully transparent or degenerate geometry — and \
                       returns no images, so it costs a fraction of a preview. \
                       Use it to verify a document is correct; use \
                       `preview_document` when you need to judge how it looks."
    )]
    pub async fn check_document(
        &self,
        Parameters(params): Parameters<crate::tools::CheckDocumentParams>,
    ) -> CallToolResult {
        match Self::check_document_impl(params) {
            Ok(result) => result,
            Err(e) => e.into_result(),
        }
    }

    fn check_document_impl(
        params: crate::tools::CheckDocumentParams,
    ) -> Result<CallToolResult, ToolError> {
        let (doc, default_base) = crate::source::load_document(
            params.document.as_deref(),
            params.document_path.as_deref(),
        )?;
        let fps = crate::source::resolve_fps(params.fps, &doc)?;
        crate::source::check_canvas_size(doc.size.w, doc.size.h)?;

        let base = params
            .asset_base_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or(default_base);

        // No Engine is built: nothing here rasterizes, so there is no reason
        // to allocate two full-canvas pixmaps. Assets are still prepared,
        // because text layout needs the fonts.
        let mut assets = crate::source::resolve_assets(&doc, &base)?;
        assets.prepare(&doc)?;

        let timeline = crate::timeline::summary(&doc);
        let total = kineto_core::timeline::total_duration(&doc);
        let moments = crate::render::resolve_moments(
            total,
            fps,
            &timeline,
            &params.at_ms,
            &params.at_scenes,
        )?;

        let document_issues = crate::check::analyze_document(&doc);
        let mut checked = Vec::with_capacity(moments.len());
        let mut issue_count = document_issues.len();
        for m in moments {
            let issues = crate::check::analyze(&doc, &mut assets, m.tick);
            issue_count += issues.len();
            checked.push(crate::tools::CheckedMoment {
                requested_ms: m.requested_ms,
                requested_scene: m.requested_scene,
                tick: m.tick,
                actual_ms: crate::render::round_ms(m.tick),
                scene_id: timeline.scene_at(m.tick).map(|s| s.id.clone()),
                issues,
            });
        }

        Ok(crate::tools::check_success(&crate::tools::CheckOutcome {
            width: doc.size.w,
            height: doc.size.h,
            fps,
            issue_count,
            document_issues,
            timeline,
            moments: checked,
        }))
    }

    #[tool(
        name = "session_append",
        description = "Record one thing that happened into a session journal. \
                       Call it as you work: a task started, a step finished, a \
                       result measured. Say what happened, not how it should \
                       look — the projection chooses that. `compile_session` \
                       later turns the journal into a watchable document."
    )]
    pub async fn session_append(
        &self,
        Parameters(params): Parameters<crate::tools::SessionAppendParams>,
    ) -> CallToolResult {
        match Self::session_append_impl(params) {
            Ok(result) => result,
            Err(e) => e.into_result(),
        }
    }

    fn session_append_impl(
        params: crate::tools::SessionAppendParams,
    ) -> Result<CallToolResult, ToolError> {
        let at_ms = match params.at_ms {
            Some(t) => t,
            // Wall-clock enters here and only here. It lands in the journal,
            // which is a log; `compile` stays a pure function of the journal.
            None => std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
        };
        let beat = crate::session::Beat {
            at_ms,
            kind: params.kind,
            title: params.title,
            detail: params.detail,
            status: params.status,
        };
        let count = crate::session::append(std::path::Path::new(&params.journal_path), &beat)?;
        Ok(CallToolResult::success(vec![
            rmcp::model::ContentBlock::text(format!(
                "recorded beat {count} in {}",
                params.journal_path
            )),
        ]))
    }

    #[tool(
        name = "compile_session",
        description = "Turn a session journal into a kineto document: one \
                       scene per beat, each given enough time to be read. \
                       Writes the document and renders nothing — pass the \
                       result to `check_document`, `preview_document` or \
                       `render_document`. Compilation is a pure function of \
                       the journal, so an old journal re-renders identically."
    )]
    pub async fn compile_session(
        &self,
        Parameters(params): Parameters<crate::tools::CompileSessionParams>,
    ) -> CallToolResult {
        match Self::compile_session_impl(params) {
            Ok(result) => result,
            Err(e) => e.into_result(),
        }
    }

    fn compile_session_impl(
        params: crate::tools::CompileSessionParams,
    ) -> Result<CallToolResult, ToolError> {
        let beats = crate::session::read(std::path::Path::new(&params.journal_path))?;
        let title = params.title.as_deref().unwrap_or("session");
        let doc = crate::session::compile(&beats, title)?;
        let json = doc.canonical_json();

        let out = std::path::Path::new(&params.out);
        if let Some(parent) = out.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| ToolError::Io {
                    context: "creating output directory",
                    path: parent.display().to_string(),
                    source: e,
                })?;
            }
        }
        std::fs::write(out, &json).map_err(|e| ToolError::Io {
            context: "writing compiled document",
            path: params.out.clone(),
            source: e,
        })?;

        // Summed from the document itself: re-deriving would be a second
        // source of truth for the same number.
        let total: i64 = doc.scenes.iter().map(|s| s.duration).sum::<i64>()
            / (kineto_core::doc::TIMEBASE / 1000);
        Ok(CallToolResult::success(vec![
            rmcp::model::ContentBlock::text(format!(
                "compiled {} beat(s) into {} — {} scenes, {:.1}s",
                beats.len(),
                params.out,
                doc.scenes.len(),
                total as f64 / 1000.0
            )),
        ]))
    }

    #[tool(
        name = "build_chart",
        description = "Turn data into a chart document: line, area or bar, \
                       with measured axes, round-number ticks and animated \
                       series. Writes the document and renders nothing — pass \
                       it to `check_document`, `preview_document` or \
                       `render_document`. The result is ordinary paths, rects \
                       and text, so it can be edited afterwards like any other \
                       document; there is no chart element in the format."
    )]
    pub async fn build_chart(
        &self,
        Parameters(params): Parameters<crate::tools::BuildChartParams>,
    ) -> CallToolResult {
        match Self::build_chart_impl(params) {
            Ok(result) => result,
            Err(e) => e.into_result(),
        }
    }

    fn build_chart_impl(
        params: crate::tools::BuildChartParams,
    ) -> Result<CallToolResult, ToolError> {
        let kind = match params.kind.as_str() {
            "line" => crate::chart::ChartKind::Line,
            "area" => crate::chart::ChartKind::Area,
            "bar" => crate::chart::ChartKind::Bar,
            other => {
                return Err(ToolError::Invalid(format!(
                    "unknown chart kind '{other}': use line, area or bar"
                )))
            }
        };
        let spec = crate::chart::ChartSpec {
            kind,
            labels: params.labels,
            series: params
                .series
                .into_iter()
                .map(|s| crate::chart::Series {
                    name: s.name,
                    values: s.values,
                    color: s.color,
                })
                .collect(),
            title: params.title,
            subtitle: params.subtitle,
            width: params.width.unwrap_or(1280),
            height: params.height.unwrap_or(720),
            seconds: params.seconds.unwrap_or(6.0),
        };
        crate::source::check_canvas_size(spec.width, spec.height)?;
        let doc = crate::chart::build(&spec)?;
        let json = doc.canonical_json();

        let out = std::path::Path::new(&params.out);
        if let Some(parent) = out.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| ToolError::Io {
                    context: "creating output directory",
                    path: parent.display().to_string(),
                    source: e,
                })?;
            }
        }
        std::fs::write(out, &json).map_err(|e| ToolError::Io {
            context: "writing chart document",
            path: params.out.clone(),
            source: e,
        })?;

        Ok(CallToolResult::success(vec![
            rmcp::model::ContentBlock::text(format!(
                "wrote {} — {} series over {} categories, {}x{}",
                params.out,
                spec.series.len(),
                spec.labels.len(),
                spec.width,
                spec.height
            )),
        ]))
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
        let timeline = crate::timeline::summary(&doc);
        let mut engine = kineto_core::Engine::new(doc, store)?;

        if params.validate_only {
            let outcome = crate::render::describe(&engine, params.fps).with_timeline(timeline);
            return Ok(crate::tools::success(&outcome, Vec::new()));
        }

        let out = params.out.ok_or_else(|| {
            ToolError::Invalid("`out` is required unless `validateOnly` is true".into())
        })?;

        let outcome =
            crate::render::render_to_file(&mut engine, params.fps, &out)?.with_timeline(timeline);
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
        let timeline = crate::timeline::summary(&doc);
        let mut engine = kineto_core::Engine::new(doc, assets)?;

        if params.validate_only {
            let outcome = crate::render::describe(&engine, params.fps).with_timeline(timeline);
            return Ok(crate::tools::success(&outcome, Vec::new()));
        }

        let out = params.out.ok_or_else(|| {
            ToolError::Invalid("`out` is required unless `validateOnly` is true".into())
        })?;

        let outcome =
            crate::render::render_to_file(&mut engine, params.fps, &out)?.with_timeline(timeline);
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
