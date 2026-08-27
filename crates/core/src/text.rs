//! Text layout via cosmic-text: a pure function from
//! `(family, text, size_px, max_w, align)` plus a prepared `FontSystem` to
//! glyph positions. No rasterization here — Task 10 blits glyphs into
//! pixels using `PlacedGlyph::cache_key` (a swash glyph cache lookup key).
//!
//! Determinism is law (spec §5): the caller's `FontSystem` must already be
//! built with an empty font database (see `assets.rs`) — this module never
//! touches system fonts, wall-clock time, or float fast-math; the layout is
//! a pure function of its inputs.

use crate::doc::Align;
use cosmic_text::{Attrs, Buffer, CacheKey, Family, FontSystem, Metrics, Shaping};

/// Line height = 1.3 × font size in pixels. Locked constant (task-7 brief).
const LINE_HEIGHT_SCALE: f32 = 1.3;

/// One glyph's physical (integer-snapped) placement, at scale 1.0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
    /// Swash glyph cache lookup key (font, glyph id, size, subpixel bin,
    /// weight, flags) — consumed by Task 10's rasterizer.
    pub cache_key: CacheKey,
    pub x: i32,
    pub y: i32,
}

/// A laid-out block of text: its bounding box plus every glyph's placement.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    pub width: f32,
    pub height: f32,
    pub glyphs: Vec<PlacedGlyph>,
}

/// Lay out `text` in `family` at `size_px`, wrapping at `max_w` if given,
/// with `align`. `fs` must already have `family`'s face loaded (see
/// `AssetStore::prepare` / `AssetStore::font_system`).
pub fn layout_text(
    fs: &mut FontSystem,
    family: &str,
    text: &str,
    size_px: f32,
    max_w: Option<f32>,
    align: Align,
) -> TextLayout {
    let line_height = size_px * LINE_HEIGHT_SCALE;
    let metrics = Metrics::new(size_px, line_height);

    let mut buffer = Buffer::new(fs, metrics);
    buffer.set_size(max_w, None);

    let attrs = Attrs::new().family(Family::Name(family));
    let cosmic_align = match align {
        Align::Left => cosmic_text::Align::Left,
        Align::Center => cosmic_text::Align::Center,
        Align::Right => cosmic_text::Align::Right,
    };
    buffer.set_text(text, &attrs, Shaping::Advanced, Some(cosmic_align));
    buffer.shape_until_scroll(fs, false);

    let mut glyphs = Vec::new();
    let mut width: f32 = 0.0;
    let mut line_count: usize = 0;
    for run in buffer.layout_runs() {
        width = width.max(run.line_w);
        line_count += 1;
        for glyph in run.glyphs {
            let placed = glyph.physical((0.0, 0.0), 1.0);
            glyphs.push(PlacedGlyph {
                cache_key: placed.cache_key,
                x: placed.x,
                y: run.line_y as i32 + placed.y,
            });
        }
    }

    TextLayout {
        width,
        height: line_count as f32 * line_height,
        glyphs,
    }
}
