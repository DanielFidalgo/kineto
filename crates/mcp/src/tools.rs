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
    /// Canonical kineto document JSON. Provide exactly one of `document` or
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

    /// Output path. The extension chooses the format: `.mp4` for h264, or
    /// `.webp` for an animated WebP — 24-bit colour and real alpha, which is
    /// what keeps gradients and soft shadows intact, and what embeds inline
    /// in markdown. Required unless `validateOnly` is true.
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
            "wrote {} ({}x{}, {} frames at {} fps, {:.3}s{})",
            path,
            outcome.width,
            outcome.height,
            outcome.frame_count,
            outcome.fps,
            outcome.duration_seconds,
            match outcome.bytes {
                Some(b) => format!(", {:.1} MB", b as f64 / 1_048_576.0),
                None => String::new(),
            }
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
pub struct PreviewDocumentParams {
    /// Canonical kineto document JSON. Provide exactly one of `document` or
    /// `documentPath`.
    #[serde(default)]
    pub document: Option<String>,

    /// Path to a `.json` document. Provide exactly one of `document` or
    /// `documentPath`. Prefer this while iterating: edit the file and preview
    /// again rather than resending the whole document each time.
    #[serde(default)]
    pub document_path: Option<String>,

    /// Directory that image and font `src` values resolve against. Defaults to
    /// the document's own directory, or the working directory for an inline
    /// document.
    #[serde(default)]
    pub asset_base_dir: Option<String>,

    /// Moments to look at, in whole milliseconds from the start of the
    /// document. Each is snapped to the frame containing it, and the frame it
    /// resolved to is reported back. Provide at least one of `atMs` or
    /// `atScenes`; at most 12 moments in total per call.
    #[serde(default)]
    pub at_ms: Vec<i64>,

    /// Scene ids to look at, each previewed at that scene's midpoint. Prefer
    /// this for long documents: it needs no arithmetic over scene durations,
    /// and it survives edits that shift the timeline. The midpoint rather than
    /// the start because a crossfaded scene is fully transparent at its own
    /// start tick, where the frame shows the previous scene instead.
    #[serde(default)]
    pub at_scenes: Vec<String>,

    /// Frames per second, which sets the frame grid moments snap to. Must be
    /// at most 1000 and divide 705600000 exactly (24, 25, 30, 50, 60...).
    /// Defaults to the document's own `defaultFps`, or 30.
    #[serde(default)]
    pub fps: Option<i64>,
}

/// The preview result: a summary, then each frame labelled with the moment it
/// answers, then the image itself.
///
/// The labels matter — a model handed three unlabelled images has to guess
/// which is which, and the whole point of the tool is that it can tell.
pub fn preview_success(
    outcome: &crate::render::PreviewOutcome,
    frames: &[u64],
    previews: Vec<String>,
) -> CallToolResult {
    let summary = format!(
        "previewed {} moment(s) of a {}x{} document ({} frames at {} fps, {:.3}s)",
        outcome.samples.len(),
        outcome.width,
        outcome.height,
        outcome.frame_count,
        outcome.fps,
        outcome.duration_seconds
    );

    let mut content = vec![ContentBlock::text(summary)];
    for (index, png) in frames.iter().zip(previews) {
        let asked: Vec<String> = outcome
            .samples
            .iter()
            .filter(|s| s.frame_index == *index)
            .map(|s| match (s.requested_ms, &s.requested_scene) {
                (Some(ms), _) => format!("{ms} ms"),
                (_, Some(id)) => format!("scene {id}"),
                _ => "?".to_string(),
            })
            .collect();
        let mut label = format!("frame {index} — requested {}", asked.join(", "));
        // Which scene the frame actually shows, which is not always the one
        // asked for: inside a crossfade the dominant scene is the neighbour.
        if let Some(s) = outcome.samples.iter().find(|s| s.frame_index == *index) {
            if let (Some(id), Some(local)) = (&s.scene_id, s.scene_local_ms) {
                label.push_str(&format!(" — showing scene {id} at {local} ms"));
            }
        }
        content.push(ContentBlock::text(label));
        content.push(ContentBlock::image(png, "image/png"));
    }

    let mut result = CallToolResult::success(content);
    result.structured_content = serde_json::to_value(outcome).ok();
    result
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckDocumentParams {
    /// Canonical kineto document JSON. Provide exactly one of `document` or
    /// `documentPath`.
    #[serde(default)]
    pub document: Option<String>,

    /// Path to a `.json` document. Provide exactly one of `document` or
    /// `documentPath`.
    #[serde(default)]
    pub document_path: Option<String>,

    /// Directory that image and font `src` values resolve against.
    #[serde(default)]
    pub asset_base_dir: Option<String>,

    /// Moments to check, in whole milliseconds from the start of the document.
    /// Provide at least one of `atMs` or `atScenes`; at most 12 in total.
    #[serde(default)]
    pub at_ms: Vec<i64>,

    /// Scene ids to check, each at that scene's midpoint.
    #[serde(default)]
    pub at_scenes: Vec<String>,

    /// Frames per second, which sets the frame grid moments snap to.
    #[serde(default)]
    pub fps: Option<i64>,
}

/// One checked moment and whatever was wrong with it.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckedMoment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_scene: Option<String>,
    pub tick: i64,
    pub actual_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_id: Option<String>,
    pub issues: Vec<crate::check::Issue>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckOutcome {
    pub width: u32,
    pub height: u32,
    pub fps: i64,
    pub issue_count: usize,
    /// Rules about the whole document, independent of any moment — reported
    /// once rather than repeated against every tick that was checked.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub document_issues: Vec<crate::check::Issue>,
    pub timeline: crate::timeline::TimelineSummary,
    pub moments: Vec<CheckedMoment>,
}

/// Text-only result. No images, deliberately: the tool exists because most
/// checks should not cost a picture.
pub fn check_success(outcome: &CheckOutcome) -> CallToolResult {
    let mut lines = Vec::new();
    if outcome.issue_count == 0 {
        lines.push(format!(
            "no issues across {} moment(s) of a {}x{} document",
            outcome.moments.len(),
            outcome.width,
            outcome.height
        ));
    } else {
        let (mut correctness, mut design) = (0usize, 0usize);
        for i in &outcome.document_issues {
            if i.category == "correctness" {
                correctness += 1
            } else {
                design += 1
            }
        }
        for m in &outcome.moments {
            for i in &m.issues {
                if i.category == "correctness" {
                    correctness += 1
                } else {
                    design += 1
                }
            }
        }
        lines.push(format!(
            "{correctness} correctness + {design} design issue(s) across {} moment(s):",
            outcome.moments.len()
        ));
        for i in &outcome.document_issues {
            lines.push(format!(
                "  document: {} [{}/{}]",
                i.detail, i.category, i.kind
            ));
        }
        for m in &outcome.moments {
            for i in &m.issues {
                let where_ = match (&i.scene, i.element) {
                    (Some(sc), Some(e)) => format!("scene '{sc}' element {e}"),
                    (Some(sc), None) => format!("scene '{sc}'"),
                    _ => "document".to_string(),
                };
                lines.push(format!(
                    "  {} ms — {where_}: {} [{}/{}]",
                    m.actual_ms, i.detail, i.category, i.kind
                ));
            }
        }
    }
    let mut result = CallToolResult::success(vec![ContentBlock::text(lines.join("\n"))]);
    result.structured_content = serde_json::to_value(outcome).ok();
    result
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionAppendParams {
    /// Path to the journal (`.jsonl`). Created if it does not exist.
    pub journal_path: String,

    /// What kind of thing happened: `task`, `step`, `result`, `note` or
    /// `error`. This chooses the accent colour; it is not a free-form label.
    pub kind: String,

    /// One line. This is the headline of the beat.
    pub title: String,

    /// An optional second line with the specifics — numbers, paths, counts.
    #[serde(default)]
    pub detail: Option<String>,

    #[serde(default)]
    pub status: Option<String>,

    /// Milliseconds since the Unix epoch. Defaults to now. Pass it explicitly
    /// to replay a session, or in tests, where a wall clock would make the
    /// compiled document differ between runs.
    #[serde(default)]
    pub at_ms: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompileSessionParams {
    /// Path to the journal written by `session_append`.
    pub journal_path: String,

    /// Where to write the compiled document JSON. Feed it to
    /// `check_document`, `preview_document` or `render_document` — this tool
    /// deliberately renders nothing itself.
    pub out: String,

    /// Shown on the progress rail of every scene.
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildChangelogParams {
    /// The headline, e.g. "Acme 2.0".
    pub title: String,
    /// A line under it.
    #[serde(default)]
    pub subtitle: Option<String>,
    /// Heading over the list of changes. Default "What changed".
    #[serde(default)]
    pub heading: Option<String>,
    /// A git range such as `v1.2.0..v1.3.0`. Defaults to everything since the
    /// previous tag.
    #[serde(default)]
    pub range: Option<String>,
    /// Which repository to read. Defaults to the working directory.
    #[serde(default)]
    pub repo: Option<String>,
    /// Lines for a closing Install scene, e.g. ["npm i acme"].
    #[serde(default)]
    pub install: Vec<String>,
    /// `midnight` (dark) or `paper` (light).
    #[serde(default)]
    pub theme: Option<String>,
    /// Where to write the document JSON. Renders nothing.
    pub out: String,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    /// How many changes to show. Default 4 — a release video is scanned, not
    /// read.
    #[serde(default)]
    pub max_points: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneParams {
    /// `title`, `points`, `code` or `quote`.
    pub kind: String,
    /// The headline for `title`, the quotation for `quote`.
    #[serde(default)]
    pub text: Option<String>,
    /// Supporting line under a `title`.
    #[serde(default)]
    pub subtitle: Option<String>,
    /// Heading above a `points` or `code` scene.
    #[serde(default)]
    pub heading: Option<String>,
    /// The bullets of a `points` scene, or the lines of a `code` scene. One
    /// entry per line; they are staggered in the order given.
    #[serde(default)]
    pub items: Vec<String>,
    /// Who said it, for a `quote`.
    #[serde(default)]
    pub attribution: Option<String>,
    /// How long this scene holds, in seconds. Default 4.
    #[serde(default)]
    pub seconds: Option<f64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildScenesParams {
    /// `midnight` (dark) or `paper` (light).
    #[serde(default)]
    pub theme: Option<String>,
    pub scenes: Vec<SceneParams>,
    /// Where to write the document JSON. Renders nothing — pass the result to
    /// `check_document`, `preview_document` or `render_document`.
    pub out: String,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildChartParams {
    /// `line`, `area` or `bar`.
    pub kind: String,

    /// One label per category, along the x axis.
    pub labels: Vec<String>,

    /// Each series needs exactly one value per label.
    pub series: Vec<ChartSeriesParams>,

    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub subtitle: Option<String>,

    /// Where to write the document JSON. Renders nothing — pass the result to
    /// `check_document`, `preview_document` or `render_document`.
    pub out: String,

    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    /// How long the chart holds. Series animate in over the first second.
    #[serde(default)]
    pub seconds: Option<f64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChartSeriesParams {
    pub name: String,
    pub values: Vec<f64>,
    /// `#RRGGBB`. Defaults to the next colour in a six-hue palette.
    #[serde(default)]
    pub color: Option<String>,
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

    /// Output path. The extension chooses the format: `.mp4` for h264, or
    /// `.webp` for an animated WebP — 24-bit colour and real alpha, which is
    /// what keeps gradients and soft shadows intact, and what embeds inline
    /// in markdown. Required unless `validateOnly` is true.
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
    pub fn resolved_theme(&self) -> kineto_asciicast::Theme {
        match &self.theme {
            Some(t) => t.apply(kineto_asciicast::Theme::default()),
            None => kineto_asciicast::Theme::default(),
        }
    }
}

impl ThemeParams {
    /// Apply the caller's overrides onto the adapter's defaults.
    pub fn apply(&self, mut theme: kineto_asciicast::Theme) -> kineto_asciicast::Theme {
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

    /// Output path. The extension chooses the format: `.mp4` for h264, or
    /// `.webp` for an animated WebP — 24-bit colour and real alpha, which is
    /// what keeps gradients and soft shadows intact, and what embeds inline
    /// in markdown. Required unless `validateOnly` is true.
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
    use kineto_asciicast::Theme;

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

    fn tiny_cast() -> kineto_asciicast::Cast {
        let header = serde_json::json!({ "version": 2, "width": 20, "height": 4 });
        kineto_asciicast::parse_cast(&format!("{header}\n[0.0, \"o\", \"hello\"]\n")).unwrap()
    }

    /// The theme override must survive the whole path, not merely be applied
    /// to a `Theme` that is then discarded: `apply` tested in isolation still
    /// passes if `cast_to_document` is handed `Theme::default()`. This starts
    /// from the wire arguments and asserts on the built document.
    #[test]
    fn an_overridden_theme_reaches_the_built_document() {
        let cast = tiny_cast();

        let (default_doc, _) = kineto_asciicast::cast_to_document(&cast, &Theme::default());
        assert_eq!(
            default_doc.bg.0, "#0A0A0A",
            "control: the adapter's own default background"
        );

        let params: RenderAsciicastParams = serde_json::from_value(serde_json::json!({
            "castPath": "/unused.cast",
            "theme": { "bg": "#101820" }
        }))
        .unwrap();
        let (doc, _) = kineto_asciicast::cast_to_document(&cast, &params.resolved_theme());

        assert_eq!(doc.bg.0, "#101820");
    }

    #[test]
    fn an_absent_theme_resolves_to_the_adapter_default() {
        let params: RenderAsciicastParams =
            serde_json::from_value(serde_json::json!({ "castPath": "/unused.cast" })).unwrap();
        assert_eq!(params.resolved_theme(), Theme::default());

        let (doc, _) = kineto_asciicast::cast_to_document(&tiny_cast(), &params.resolved_theme());
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
