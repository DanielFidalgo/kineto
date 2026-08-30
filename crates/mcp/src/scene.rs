//! A vocabulary of well-composed scenes.
//!
//! Deliberately a vocabulary and not a generator. Each kind here is an
//! independent builder — the same shape as `chart::build` — and the caller
//! still chooses the sequence, the durations and the transitions, and can edit
//! the document afterwards. Nothing here decides what a video is *about*. A
//! tool that took a title and a list of sections and returned a finished
//! explainer would be a product built on Kineto; this is the layer under that.
//!
//! Every position and size comes from [`Theme`], so a scene is laid out by
//! arithmetic rather than by a model guessing coordinates — which is what
//! makes the output look composed rather than assembled.
//!
//! Motion is emitted as `reveal` sugar and expanded by [`crate::motion`] on
//! load. Writing keyframes here would duplicate that, and quietly diverge.

use serde_json::{json, Value};

use crate::error::ToolError;
use crate::theme::Theme;

/// Entrance stagger between sibling lines. Long enough to read as sequence,
/// short enough that six lines do not outlast the scene.
const STAGGER_MS: i64 = 110;

/// Everything enters within this budget, so a scene's dwell time is what the
/// caller asked for minus a constant, not minus something that grows with the
/// number of lines.
const FIRST_MS: i64 = 60;

pub const KINDS: &[&str] = &["title", "points", "code", "quote"];

/// Approximate advance width of Inter at a given size, as a fraction of em.
///
/// Only used to decide whether a line *will* overflow so it can be wrapped by
/// `maxW` rather than run off the canvas. Deliberately an estimate: the real
/// measurement lives in the shaper, and a builder that had to shape text to
/// lay it out would need fonts loaded, which is a much heavier contract than
/// "emit a document".
const AVG_GLYPH_EM: f64 = 0.52;

fn est_width(text: &str, size_px: f64) -> f64 {
    text.chars().count() as f64 * size_px * AVG_GLYPH_EM
}

/// One line of type: what it says and how it looks, separate from where it
/// goes.
struct Line<'a> {
    body: &'a str,
    font: &'a str,
    size: f64,
    color: &'a kineto_core::Color,
    at_ms: i64,
    kind: &'a str,
}

/// A `text` element, with `maxW` set whenever the line might not fit.
fn text(l: Line, x: f64, y: f64, max_w: f64) -> Value {
    let Line {
        body,
        font,
        size,
        color,
        at_ms,
        kind,
    } = l;
    let mut el = json!({
        "type": "text", "text": body, "font": font,
        "sizePx": size, "color": color.0, "pos": [x, y],
        "reveal": { "at": at_ms, "kind": kind }
    });
    // Only set maxW when wrapping is actually needed: an unnecessary maxW
    // changes nothing visually but shows up in every emitted document.
    if est_width(body, size) > max_w {
        el["maxW"] = json!(max_w);
    }
    el
}

/// Top edge for a content block of `block_h`, so it sits on the optical
/// centre.
///
/// Anchoring content at a fixed fraction of the canvas is what makes generated
/// video look like a slide: a three-line list and a nine-line list both start
/// at 20% and leave wildly different amounts of dead space below. Centring the
/// block instead means the composition is balanced whatever the content is.
///
/// Biased slightly above true centre because a block centred by arithmetic
/// reads as low — the same reason a title in print sits above the midline.
fn optical_top(t: &Theme, block_h: f64) -> f64 {
    ((t.height - block_h) / 2.0 - t.height * 0.045).max(t.height * 0.10)
}

/// Height a line of text occupies, including its leading.
fn line_h(size_px: f64) -> f64 {
    size_px * 1.25
}

/// The content of one scene, before layout.
pub struct SceneSpec {
    pub kind: String,
    pub text: Option<String>,
    pub subtitle: Option<String>,
    pub heading: Option<String>,
    pub items: Vec<String>,
    pub attribution: Option<String>,
    /// Absent means "long enough to read" — see [`default_seconds`].
    pub seconds: Option<f64>,
}

/// Builds one scene object for a document's `scenes` array.
/// How long a scene needs, when the caller does not say.
///
/// Sized by the same rule `check::check_scene` enforces, using the same
/// constants — a builder that emitted scenes its own linter called `tooFast`
/// was the first thing the test caught, and hard-coding four seconds is
/// exactly the guess this vocabulary exists to remove. Entrance time is added
/// on top, because text cannot be read while it is still arriving.
pub fn default_seconds(spec: &SceneSpec) -> f64 {
    let words: usize = spec
        .text
        .iter()
        .chain(spec.subtitle.iter())
        .chain(spec.heading.iter())
        .chain(spec.attribution.iter())
        .chain(spec.items.iter())
        .map(|s| s.split_whitespace().count())
        .sum();

    let lines = spec.items.len() as i64 + 2;
    let entrance_ms = (FIRST_MS + STAGGER_MS * lines) as f64;
    let read_ms = words as f64 / crate::check::SCAN_WPM * 60_000.0 + crate::check::SCENE_BEAT_MS;

    // A tenth over the threshold rather than exactly on it: the linter's
    // measurement counts the words actually rendered, which can exceed what
    // this sees if a kind adds chrome of its own.
    let ms = (read_ms + entrance_ms) * 1.1;
    (ms / 1000.0).max(2.5)
}

pub fn build(spec: &SceneSpec, theme: &Theme, index: usize) -> Result<Value, ToolError> {
    let seconds = match spec.seconds {
        Some(s) if s <= 0.0 => {
            return Err(ToolError::DocumentSource(format!(
                "scene {index}: seconds must be positive, got {s}"
            )))
        }
        Some(s) => s,
        None => default_seconds(spec),
    };
    let elements = match spec.kind.as_str() {
        "title" => title(spec, theme)?,
        "points" => points(spec, theme)?,
        "code" => code(spec, theme)?,
        "quote" => quote(spec, theme)?,
        other => {
            return Err(ToolError::DocumentSource(format!(
                "scene {index}: unknown kind '{other}': use one of {}",
                KINDS.join(", ")
            )))
        }
    };
    let ticks = (seconds * kineto_core::doc::TIMEBASE as f64).round() as i64;
    Ok(json!({
        "id": format!("{}-{index}", spec.kind),
        "duration": ticks,
        "elements": elements,
    }))
}

fn require<'a>(v: &'a Option<String>, field: &str, kind: &str) -> Result<&'a str, ToolError> {
    v.as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            ToolError::DocumentSource(format!("a '{kind}' scene needs a non-empty '{field}'"))
        })
}

/// Title and optional subtitle, sitting on the optical centre line.
///
/// Placed slightly above true centre: text centred by its own bounding box
/// reads as low, which is why every title card in print sits high.
fn title(spec: &SceneSpec, t: &Theme) -> Result<Vec<Value>, ToolError> {
    let headline = require(&spec.text, "text", "title")?;
    let has_sub = spec
        .subtitle
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());
    let block_h = t.block_gap
        + line_h(t.title_px)
        + if has_sub {
            t.line_gap * 0.4 + line_h(t.heading_px)
        } else {
            0.0
        };
    let y = optical_top(t, block_h) + t.block_gap;
    let mut out = vec![
        // A short accent rule above the title, which is what stops a title
        // card reading as a bare string on a background.
        json!({
            "type": "rect",
            "rect": [t.margin, y - t.block_gap, t.width * 0.09, (t.height * 0.008).max(3.0)],
            "radius": (t.height * 0.004).max(1.5),
            "fill": {
                "type": "linear", "from": [0, 0], "to": [1, 0],
                "stops": [
                    { "at": 0, "color": t.accent.0 },
                    { "at": 1, "color": t.accent_alt.0 }
                ]
            },
            "reveal": { "at": FIRST_MS, "kind": "slideRight", "distance": t.width * 0.03 }
        }),
        text(
            Line {
                body: headline,
                font: "display",
                size: t.title_px,
                color: &t.fg,
                at_ms: FIRST_MS + STAGGER_MS,
                kind: "fadeUp",
            },
            t.margin - t.title_px * 0.06,
            y,
            t.content_width(),
        ),
    ];
    if let Some(sub) = spec.subtitle.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push(text(
            Line {
                body: sub,
                font: "display",
                size: t.heading_px,
                color: &t.muted,
                at_ms: FIRST_MS + STAGGER_MS * 2,
                kind: "fadeUp",
            },
            t.margin,
            y + line_h(t.title_px) + t.line_gap * 0.4,
            t.content_width(),
        ));
    }
    Ok(out)
}

/// A heading and a staggered list. The stagger is the point: five lines
/// appearing together is a slide, five arriving in sequence is a video.
fn points(spec: &SceneSpec, t: &Theme) -> Result<Vec<Value>, ToolError> {
    if spec.items.is_empty() {
        return Err(ToolError::DocumentSource(
            "a 'points' scene needs at least one item".into(),
        ));
    }
    let mut out = Vec::new();
    let has_head = spec
        .heading
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());
    let head_h = if has_head {
        line_h(t.heading_px) + t.block_gap * 0.6
    } else {
        0.0
    };
    let block_h = head_h + t.line_gap * spec.items.len() as f64;
    let mut y = optical_top(t, block_h);
    let mut at = FIRST_MS;

    if let Some(h) = spec.heading.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push(text(
            Line {
                body: h,
                font: "display",
                size: t.heading_px,
                color: &t.fg,
                at_ms: at,
                kind: "fadeUp",
            },
            t.margin,
            y,
            t.content_width(),
        ));
        y += head_h;
        at += STAGGER_MS;
    }

    for item in &spec.items {
        // A small accent marker, vertically centred on the line's cap height.
        let dot = (t.body_px * 0.24).max(3.0);
        out.push(json!({
            "type": "rect",
            "rect": [t.margin, y + t.body_px * 0.42, dot, dot],
            "radius": dot / 2.0,
            "fill": t.accent.0,
            "reveal": { "at": at, "kind": "popIn" }
        }));
        out.push(text(
            Line {
                body: item,
                font: "display",
                size: t.body_px,
                color: &t.fg,
                at_ms: at,
                kind: "fadeUp",
            },
            t.margin + dot * 3.0,
            y,
            t.content_width() - dot * 3.0,
        ));
        y += t.line_gap;
        at += STAGGER_MS;
    }
    Ok(out)
}

/// Monospaced lines on a surface panel. The panel is what makes code read as
/// code before a single character is parsed.
fn code(spec: &SceneSpec, t: &Theme) -> Result<Vec<Value>, ToolError> {
    if spec.items.is_empty() {
        return Err(ToolError::DocumentSource(
            "a 'code' scene needs at least one line in 'lines'".into(),
        ));
    }
    let mut out = Vec::new();
    let has_head = spec
        .heading
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());
    let head_h = if has_head {
        line_h(t.heading_px) + t.block_gap * 0.6
    } else {
        0.0
    };
    let pad = t.height * 0.045;
    let code_line = t.body_px * 1.7;
    let panel_h = code_line * spec.items.len() as f64 + pad * 2.0;

    let mut y = optical_top(t, head_h + panel_h);
    let mut at = FIRST_MS;

    if let Some(h) = spec.heading.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push(text(
            Line {
                body: h,
                font: "display",
                size: t.heading_px,
                color: &t.fg,
                at_ms: at,
                kind: "fadeUp",
            },
            t.margin,
            y,
            t.content_width(),
        ));
        y += head_h;
        at += STAGGER_MS;
    }
    out.push(json!({
        "type": "rect",
        "rect": [t.margin, y, t.content_width(), panel_h],
        "radius": t.height * 0.018,
        "fill": t.surface.0,
        "reveal": { "at": at, "kind": "fadeUp" }
    }));

    let mut line_y = y + pad;
    for line in &spec.items {
        at += STAGGER_MS / 2;
        out.push(text(
            Line {
                body: line,
                font: "mono",
                size: t.body_px,
                color: &t.fg,
                at_ms: at,
                kind: "fadeUp",
            },
            t.margin + pad,
            line_y,
            t.content_width() - pad * 2.0,
        ));
        line_y += code_line;
    }
    Ok(out)
}

/// A pulled quote: larger than body, with the accent carried by a left rule
/// rather than by colouring the text, which would cost contrast.
fn quote(spec: &SceneSpec, t: &Theme) -> Result<Vec<Value>, ToolError> {
    let body = require(&spec.text, "text", "quote")?;
    let has_attr = spec
        .attribution
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());
    // The rule should match the quotation it marks, so estimate how many
    // lines the text will wrap to. An estimate rather than a measurement for
    // the reason `est_width` exists: shaping here would mean loading fonts.
    // Erring long would leave the rule hanging past the text, so this rounds
    // up only when a line is comfortably over.
    let rule_w = (t.width * 0.005).max(3.0);
    let avail = t.width - (t.margin + rule_w * 4.0) - t.margin;
    let lines = (est_width(body, t.heading_px) / avail).ceil().max(1.0);
    let quote_h = line_h(t.heading_px) * lines;
    let block_h = quote_h
        + if has_attr {
            t.block_gap + line_h(t.caption_px)
        } else {
            0.0
        };
    let y = optical_top(t, block_h);
    let x = t.margin + rule_w * 4.0;
    let width = t.width - x - t.margin;

    let mut out = vec![
        json!({
            "type": "rect",
            "rect": [t.margin, y, rule_w, quote_h],
            "radius": rule_w / 2.0,
            "fill": t.accent.0,
            "reveal": { "at": FIRST_MS, "kind": "fadeDown", "distance": t.height * 0.06 }
        }),
        text(
            Line {
                body,
                font: "display",
                size: t.heading_px,
                color: &t.fg,
                at_ms: FIRST_MS + STAGGER_MS,
                kind: "fadeUp",
            },
            x,
            y,
            width,
        ),
    ];
    if let Some(who) = spec.attribution.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push(text(
            Line {
                body: who,
                font: "display",
                size: t.caption_px,
                color: &t.muted,
                at_ms: FIRST_MS + STAGGER_MS * 2,
                kind: "fadeUp",
            },
            x,
            y + quote_h + t.block_gap * 0.5,
            width,
        ));
    }
    Ok(out)
}

/// Assembles a whole document from a theme and a list of scene specs.
pub fn build_document(
    theme_name: &str,
    width: u32,
    height: u32,
    specs: &[SceneSpec],
) -> Result<String, ToolError> {
    if specs.is_empty() {
        return Err(ToolError::DocumentSource("no scenes given".into()));
    }
    let theme = Theme::resolve(theme_name, width as f64, height as f64)?;
    let scenes: Vec<Value> = specs
        .iter()
        .enumerate()
        .map(|(i, s)| build(s, &theme, i))
        .collect::<Result<_, _>>()?;

    let doc = json!({
        "v": 1,
        "timebase": kineto_core::doc::TIMEBASE,
        "defaultFps": 30,
        "size": { "w": width, "h": height },
        "bg": theme.bg.0,
        "assets": {
            "display": { "type": "font", "src": "kineto:inter" },
            "mono": { "type": "font", "src": "kineto:jetbrains-mono" }
        },
        "scenes": scenes,
    });
    serde_json::to_string_pretty(&doc)
        .map_err(|e| ToolError::DocumentSource(format!("serialising document: {e}")))
}
