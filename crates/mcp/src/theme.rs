//! Design tokens: a palette, a type scale, and a spacing rhythm.
//!
//! An agent handed a blank document invents canvas size, colours, type sizes
//! and margins from nothing, per document. Those choices are most of the
//! difference between "looks designed" and "looks like JSON that got
//! rendered", and inventing them fresh each time produces consistent
//! mediocrity — which is the whole "boring slide deck" complaint.
//!
//! The fix is not better numbers, it is *derived* numbers. Everything here is
//! a ratio of the canvas, so a 1920x1080 deck and a 960x400 banner are the
//! same design at two sizes, and neither depends on a model picking 54 rather
//! than 48. That is the part a language model cannot do reliably and simple
//! arithmetic can.
//!
//! This is a vocabulary, not a product. It supplies proportions and colour;
//! it has no opinion about what a video says.

use kineto_core::Color;

use crate::error::ToolError;

/// A resolved theme: absolute values for one canvas size.
#[derive(Clone, Debug)]
pub struct Theme {
    pub name: &'static str,
    pub bg: Color,
    /// Primary text. Carries the contrast against `bg`.
    pub fg: Color,
    /// Secondary text — subtitles, captions, anything supporting.
    pub muted: Color,
    /// The one saturated colour. Used sparingly, which is what makes it read
    /// as an accent rather than decoration.
    pub accent: Color,
    /// Second accent, for gradients and two-tone rules.
    pub accent_alt: Color,
    /// Panels and code backgrounds: a step away from `bg`, not a new colour.
    pub surface: Color,

    // Type scale. Ratios of canvas height, so text keeps its proportion at
    // any size rather than becoming unreadable on a small canvas.
    pub title_px: f64,
    pub heading_px: f64,
    pub body_px: f64,
    pub caption_px: f64,

    // Spacing rhythm.
    /// Left and right margin.
    pub margin: f64,
    /// Vertical distance between successive lines of body text.
    pub line_gap: f64,
    /// Vertical distance between blocks.
    pub block_gap: f64,
    pub width: f64,
    pub height: f64,
}

/// Type sizes as fractions of canvas height.
///
/// `body` is deliberately well above `check`'s legibility floor (1.6% of
/// height): a vocabulary that emits documents its own linter complains about
/// would be worse than no vocabulary.
/// These are deliberately larger than print proportions. Video is read at a
/// glance and often small, so body text at 2.8% of height — a perfectly
/// reasonable page — looks thin and timid in a frame. Broadcast captions sit
/// nearer 4%, and matching that is most of what makes a frame feel filled
/// rather than sparse.
const TITLE: f64 = 0.098;
const HEADING: f64 = 0.056;
const BODY: f64 = 0.038;
const CAPTION: f64 = 0.026;

const MARGIN: f64 = 0.068;
const LINE_GAP: f64 = 0.078;
const BLOCK_GAP: f64 = 0.085;

pub const NAMES: &[&str] = &["midnight", "paper"];

/// The themes here are compiled-in constants, so an invalid one is a bug in
/// this file rather than bad input — but `rgba8` only debug-asserts validity,
/// so catching it here keeps a typo from reaching the rasteriser.
fn color(hex: &str) -> Color {
    debug_assert!(
        Color::parse_ok(hex),
        "built-in theme colour {hex} is invalid"
    );
    Color(hex.to_string())
}

impl Theme {
    /// Resolves a named theme against a canvas.
    pub fn resolve(name: &str, width: f64, height: f64) -> Result<Theme, ToolError> {
        let (bg, fg, muted, accent, accent_alt, surface) = match name {
            // The palette the project already uses for its own hero and social
            // card, so themed output looks like Kineto rather than like a
            // default.
            "midnight" => (
                "#0B1116", "#F4F7F9", "#8FA3B0", "#FF9F45", "#C77DFF", "#16232F",
            ),
            "paper" => (
                "#F7F5F2", "#14181C", "#5C6670", "#C2410C", "#7C3AED", "#E8E4DE",
            ),
            other => {
                return Err(ToolError::DocumentSource(format!(
                    "unknown theme '{other}': use one of {}",
                    NAMES.join(", ")
                )))
            }
        };
        Ok(Theme {
            name: if name == "paper" { "paper" } else { "midnight" },
            bg: color(bg),
            fg: color(fg),
            muted: color(muted),
            accent: color(accent),
            accent_alt: color(accent_alt),
            surface: color(surface),
            title_px: (height * TITLE).round(),
            heading_px: (height * HEADING).round(),
            body_px: (height * BODY).round(),
            caption_px: (height * CAPTION).round(),
            margin: (width * MARGIN).round(),
            line_gap: (height * LINE_GAP).round(),
            block_gap: (height * BLOCK_GAP).round(),
            width,
            height,
        })
    }

    /// Width available for content between the margins.
    pub fn content_width(&self) -> f64 {
        self.width - self.margin * 2.0
    }
}
