//! Every failure the tools can produce, and its mapping onto MCP.
//!
//! All of these are *tool-level* errors: the request was well-formed and
//! routed correctly, and the caller needs to read the message to fix their
//! input. Per rmcp's own guidance, that means `Ok(CallToolResult::error(..))`
//! — an `Err(ErrorData)` is rendered opaquely by MCP clients and the message
//! never reaches the model.

use rmcp::model::{CallToolResult, ContentBlock};
use zoetrope_core::DocError;

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("{0}")]
    DocumentSource(String),

    #[error("invalid document: {0}")]
    Document(#[from] DocError),

    #[error("asset '{id}': {source} (resolved to {path})")]
    Asset {
        id: String,
        path: String,
        source: std::io::Error,
    },

    #[error("{context}: {source} ({path})")]
    Io {
        context: &'static str,
        path: String,
        source: std::io::Error,
    },

    #[error(
        "unsupported fps {0}: fps must be positive and divide the timebase \
         705600000 exactly (e.g. 24, 25, 30, 50, 60)"
    )]
    Fps(i64),

    #[error(
        "ffmpeg was not found on PATH. zoetrope renders frames itself but \
         relies on ffmpeg to encode and mux MP4. Install it (macOS: \
         `brew install ffmpeg`; Debian/Ubuntu: `apt install ffmpeg`) and retry."
    )]
    FfmpegMissing,

    #[error(
        "ffmpeg is installed but failed while muxing to {0}. Its stderr was \
         inherited by this server's stderr; check the host logs for the cause."
    )]
    MuxFailed(String),

    #[error("{0}")]
    Invalid(String),
}

impl ToolError {
    pub fn into_result(self) -> CallToolResult {
        CallToolResult::error(vec![ContentBlock::text(self.to_string())])
    }
}
