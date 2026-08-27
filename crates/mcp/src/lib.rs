//! MCP server exposing the native zoetrope engine over stdio.

use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::ServerHandler;

#[derive(Clone, Default)]
pub struct ZoetropeServer {}

impl ZoetropeServer {
    pub fn new() -> Self {
        ZoetropeServer {}
    }
}

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

impl ServerHandler for ZoetropeServer {
    fn get_info(&self) -> ServerInfo {
        server_info(ServerCapabilities::builder().build())
    }
}
