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

    /// Frames per second. Must be at most 1000 and divide 705600000 exactly
    /// (24, 25, 30, 50, 60...). Defaults to the document's own `defaultFps`,
    /// or 30 if it declares none.
    #[serde(default)]
    pub fps: Option<i64>,

    /// Parse and validate the document without rendering any frames. Every
    /// referenced image and font is still read from disk and decoded, so a
    /// missing or corrupt asset is reported here.
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
    let summary = match &outcome.out {
        Some(path) => format!(
            "wrote {} ({}x{}, {} frames at {} fps, {:.3}s)",
            path,
            outcome.width,
            outcome.height,
            outcome.frame_count,
            outcome.fps,
            outcome.duration_seconds
        ),
        None => format!(
            "document is valid: {}x{}, {} frames at {} fps ({:.3}s)",
            outcome.width,
            outcome.height,
            outcome.frame_count,
            outcome.fps,
            outcome.duration_seconds
        ),
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

    /// Frames per second. Must be at most 1000 and divide 705600000 exactly.
    #[serde(default = "default_fps")]
    pub fps: i64,

    /// Terminal colors and font size. Cell metrics are deliberately not
    /// exposed: they are coupled to the bundled monospace font's advance
    /// width, and overriding them produces misaligned output.
    #[serde(default)]
    pub theme: Option<ThemeParams>,

    /// Parse and convert without rendering any frames. The bundled terminal
    /// font is still loaded and decoded.
    #[serde(default)]
    pub validate_only: bool,

    /// How many evenly spaced frames to return as inline images. 0 disables;
    /// capped at 12.
    #[serde(default = "default_preview_frames")]
    pub preview_frames: usize,
}

impl RenderAsciicastParams {
    /// The theme `cast_to_document` is actually handed.
    ///
    /// A method rather than three lines inside the tool body so a test can
    /// drive it from deserialized wire arguments: `ThemeParams::apply` tested
    /// alone proves nothing about whether its result is ever used.
    pub fn resolved_theme(&self) -> zoetrope_asciicast::Theme {
        match &self.theme {
            Some(t) => t.apply(zoetrope_asciicast::Theme::default()),
            None => zoetrope_asciicast::Theme::default(),
        }
    }
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

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoryboardFrameParams {
    /// Path to a PNG or JPEG image.
    pub image: String,
    /// How long this frame is held, in milliseconds. Must be between 1 and
    /// 86400000 (24 hours).
    pub duration_ms: i64,
    /// Optional caption, drawn in a band across the bottom of the frame.
    #[serde(default)]
    pub caption: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenderStoryboardParams {
    /// Ordered frames. Must not be empty, and at most 10000 long.
    pub frames: Vec<StoryboardFrameParams>,

    /// Output `.mp4` path. Required unless `validateOnly` is true.
    #[serde(default)]
    pub out: Option<String>,

    /// Frames per second. Must be at most 1000 and divide 705600000 exactly.
    #[serde(default = "default_fps")]
    pub fps: i64,

    /// Canvas width in pixels. Defaults to the first image's width.
    #[serde(default)]
    pub width: Option<u32>,

    /// Canvas height in pixels. Defaults to the first image's height.
    #[serde(default)]
    pub height: Option<u32>,

    /// Build and validate without rendering any frames. Every image is still
    /// read from disk and decoded, so a missing or corrupt one is reported
    /// here.
    #[serde(default)]
    pub validate_only: bool,

    /// How many evenly spaced frames to return as inline images. 0 disables;
    /// capped at 12.
    #[serde(default = "default_preview_frames")]
    pub preview_frames: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoetrope_asciicast::Theme;

    #[test]
    fn apply_overrides_only_the_set_fields() {
        let params = ThemeParams {
            bg: Some("#101820".to_string()),
            fg: None,
            size_px: None,
        };
        let theme = params.apply(Theme::default());
        let default = Theme::default();

        assert_eq!(theme.bg, "#101820");
        assert_eq!(theme.fg, default.fg);
        assert_eq!(theme.size_px, default.size_px);
        assert_eq!(theme.cell_w, default.cell_w);
        assert_eq!(theme.cell_h, default.cell_h);
        assert_eq!(theme.pad, default.pad);
    }

    fn tiny_cast() -> zoetrope_asciicast::Cast {
        let header = serde_json::json!({ "version": 2, "width": 20, "height": 4 });
        zoetrope_asciicast::parse_cast(&format!("{header}\n[0.0, \"o\", \"hello\"]\n")).unwrap()
    }

    /// The theme override must survive the whole path, not merely be applied
    /// to a `Theme` that is then discarded: `apply` tested in isolation still
    /// passes if `cast_to_document` is handed `Theme::default()`. This starts
    /// from the wire arguments and asserts on the built document.
    #[test]
    fn an_overridden_theme_reaches_the_built_document() {
        let cast = tiny_cast();

        let (default_doc, _) = zoetrope_asciicast::cast_to_document(&cast, &Theme::default());
        assert_eq!(
            default_doc.bg.0, "#0A0A0A",
            "control: the adapter's own default background"
        );

        let params: RenderAsciicastParams = serde_json::from_value(serde_json::json!({
            "castPath": "/unused.cast",
            "theme": { "bg": "#101820" }
        }))
        .unwrap();
        let (doc, _) = zoetrope_asciicast::cast_to_document(&cast, &params.resolved_theme());

        assert_eq!(doc.bg.0, "#101820");
    }

    #[test]
    fn an_absent_theme_resolves_to_the_adapter_default() {
        let params: RenderAsciicastParams =
            serde_json::from_value(serde_json::json!({ "castPath": "/unused.cast" })).unwrap();
        assert_eq!(params.resolved_theme(), Theme::default());

        let (doc, _) = zoetrope_asciicast::cast_to_document(&tiny_cast(), &params.resolved_theme());
        assert_eq!(doc.bg.0, "#0A0A0A");
    }

    #[test]
    fn apply_with_no_overrides_is_identical_to_default() {
        let params = ThemeParams {
            bg: None,
            fg: None,
            size_px: None,
        };
        let theme = params.apply(Theme::default());
        assert_eq!(theme, Theme::default());
    }
}
