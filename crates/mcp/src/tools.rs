//! The MCP tool surface. Parameter structs derive `JsonSchema` so the wire
//! schema is generated from these types rather than hand-maintained.

use rmcp::model::{CallToolResult, ContentBlock};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::render::RenderOutcome;

pub fn default_fps() -> i64 {
    30
}
pub fn default_preview_frames() -> usize {
    5
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenderDocumentParams {
    /// Canonical zoetrope document JSON. Provide exactly one of `document` or
    /// `documentPath`.
    #[serde(default)]
    pub document: Option<String>,

    /// Path to a `.json` document. Provide exactly one of `document` or
    /// `documentPath`.
    #[serde(default)]
    pub document_path: Option<String>,

    /// Directory that image and font `src` values resolve against. Defaults to
    /// the document's own directory, or the working directory for an inline
    /// document.
    #[serde(default)]
    pub asset_base_dir: Option<String>,

    /// Output `.mp4` path. Required unless `validateOnly` is true.
    #[serde(default)]
    pub out: Option<String>,

    /// Frames per second. Must divide 705600000 exactly (24, 25, 30, 50, 60...).
    #[serde(default = "default_fps")]
    pub fps: i64,

    /// Parse and validate the document without rendering anything.
    #[serde(default)]
    pub validate_only: bool,

    /// How many evenly spaced frames to return as inline images, so the caller
    /// can check the result. 0 disables; capped at 12.
    #[serde(default = "default_preview_frames")]
    pub preview_frames: usize,
}

/// The shared success shape for every render tool: a one-line summary, the
/// structured metadata, then the sampled frames.
pub fn success(outcome: &RenderOutcome, previews: Vec<String>) -> CallToolResult {
    let summary = if outcome.out.is_empty() {
        format!(
            "document is valid: {}x{}, {} frames at {} fps ({:.3}s)",
            outcome.width,
            outcome.height,
            outcome.frame_count,
            outcome.fps,
            outcome.duration_seconds
        )
    } else {
        format!(
            "wrote {} ({}x{}, {} frames at {} fps, {:.3}s)",
            outcome.out,
            outcome.width,
            outcome.height,
            outcome.frame_count,
            outcome.fps,
            outcome.duration_seconds
        )
    };

    let mut content = vec![ContentBlock::text(summary)];
    for png in previews {
        content.push(ContentBlock::image(png, "image/png"));
    }

    let mut result = CallToolResult::success(content);
    result.structured_content = serde_json::to_value(outcome).ok();
    result
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeParams {
    /// Background color, `#RRGGBB`.
    #[serde(default)]
    pub bg: Option<String>,
    /// Foreground (text) color, `#RRGGBB`.
    #[serde(default)]
    pub fg: Option<String>,
    /// Font size in pixels.
    #[serde(default)]
    pub size_px: Option<f64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenderAsciicastParams {
    /// Path to an asciicast v2 `.cast` file.
    pub cast_path: String,

    /// Output `.mp4` path. Required unless `validateOnly` is true.
    #[serde(default)]
    pub out: Option<String>,

    /// Frames per second. Must divide 705600000 exactly.
    #[serde(default = "default_fps")]
    pub fps: i64,

    /// Terminal colors and font size. Cell metrics are deliberately not
    /// exposed: they are coupled to the bundled monospace font's advance
    /// width, and overriding them produces misaligned output.
    #[serde(default)]
    pub theme: Option<ThemeParams>,

    /// Parse and convert without rendering anything.
    #[serde(default)]
    pub validate_only: bool,

    /// How many evenly spaced frames to return as inline images. 0 disables;
    /// capped at 12.
    #[serde(default = "default_preview_frames")]
    pub preview_frames: usize,
}

impl ThemeParams {
    /// Apply the caller's overrides onto the adapter's defaults.
    pub fn apply(&self, mut theme: zoetrope_asciicast::Theme) -> zoetrope_asciicast::Theme {
        if let Some(bg) = &self.bg {
            theme.bg = bg.clone();
        }
        if let Some(fg) = &self.fg {
            theme.fg = fg.clone();
        }
        if let Some(size) = self.size_px {
            theme.size_px = size;
        }
        theme
    }
}
