//! Rasterizer: canvas setup, center-based element transforms, and per-element
//! drawing. Task 8 wires the `Rect` element only — `Image`/`Text`/`Group`
//! arms exist (so match is exhaustive as the doc model grows) but are
//! implemented in later tasks (9/10/11 respectively).
//!
//! Determinism is law (spec §5): drawing is a pure function of
//! `(elements, assets, t, offset)` — no wall-clock time, no system fonts, no
//! fast-math. `tiny-skia`'s own integer/float math is what makes native and
//! wasm output byte-identical (see `assets.rs` for the analogous argument on
//! image decode).

use crate::anim::{resolve_common, Resolved};
use crate::assets::AssetStore;
use crate::doc::Element;
use tiny_skia::{
    BlendMode, FillRule, FilterQuality, Paint, PathBuilder, PixmapMut, PixmapPaint, Rect as SkRect,
    Shader, Transform,
};

/// Axis-aligned base bounding box, in the element's parent-local coordinate
/// space (i.e. before that element's own `translate/scale/rotation`, but
/// after any static `origin` shift already baked in — see `base_bbox`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Build the transform for one element's animated properties, rotating and
/// scaling about the box's own center (spec §3.3: base geometry is static;
/// only translate/scale/rotation/opacity animate, and rotation/scale pivot
/// on the element's center, not its origin corner).
///
/// Intended math: `M = T(translate) · T(center) · R(rotation) · S(scale) ·
/// T(−center)`, read right-to-left as it applies to a point: shift the box
/// so its center is at the origin, scale, rotate, then shift back to the
/// center and add the animated translate. `tiny_skia::Transform`'s `post_*`
/// builders post-concat (apply *after* the existing transform), so chaining
/// them in that same top-to-bottom order reproduces exactly that matrix.
pub fn element_matrix(b: &BBox, r: &Resolved) -> Transform {
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    Transform::from_translate(-cx, -cy)
        .post_scale(r.scale as f32, r.scale as f32)
        .post_rotate(r.rotation as f32)
        .post_translate(cx + r.translate.0 as f32, cy + r.translate.1 as f32)
}

/// Static base bounding box of an element's own geometry, ignoring its own
/// (and any descendant's) `translate/scale/rotation` — those are animated
/// and applied at draw time via `element_matrix`. Group boxes are the union
/// of their children's base boxes, shifted by the group's static `origin`
/// (recursing through nested groups' origins the same way).
///
/// Text's box is a placeholder — `pos` with zero size — until Task 10 wires
/// in the real `layout_text` box (glyph extents depend on the font, which
/// this function doesn't have access to).
pub fn base_bbox(el: &Element) -> BBox {
    match el {
        Element::Image { rect, .. } | Element::Rect { rect, .. } => BBox {
            x: rect[0].0 as f32,
            y: rect[1].0 as f32,
            w: rect[2].0 as f32,
            h: rect[3].0 as f32,
        },
        Element::Text { pos, .. } => BBox {
            x: pos[0].0 as f32,
            y: pos[1].0 as f32,
            w: 0.0,
            h: 0.0,
        },
        Element::Group {
            origin, children, ..
        } => {
            let (ox, oy) = (origin[0].0 as f32, origin[1].0 as f32);
            let mut union: Option<BBox> = None;
            for child in children {
                let b = base_bbox(child);
                let shifted = BBox {
                    x: b.x + ox,
                    y: b.y + oy,
                    w: b.w,
                    h: b.h,
                };
                union = Some(match union {
                    None => shifted,
                    Some(u) => union_bbox(u, shifted),
                });
            }
            union.unwrap_or(BBox {
                x: ox,
                y: oy,
                w: 0.0,
                h: 0.0,
            })
        }
    }
}

fn union_bbox(a: BBox, b: BBox) -> BBox {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.w).max(b.x + b.w);
    let y1 = (a.y + a.h).max(b.y + b.h);
    BBox {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

/// Renders elements to pixels. Owns the `swash` glyph raster cache (Task 10)
/// so repeated calls across frames reuse rasterized glyph bitmaps instead of
/// re-rasterizing every frame.
pub struct Renderer {
    pub swash: cosmic_text::SwashCache,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    pub fn new() -> Self {
        Renderer {
            swash: cosmic_text::SwashCache::new(),
        }
    }

    /// Draw `elements` (already in document/paint order, spec §3.3) onto
    /// `canvas` at time `t`, with `offset` the accumulated parent-group
    /// origin (scene root passes `(0.0, 0.0)`).
    // `assets` is unused until the Image (Task 9) and Text (Task 10) arms
    // below are implemented; kept named (not `_assets`) since it's part of
    // this fn's load-bearing public signature, not a truly dead param.
    #[allow(unused_variables)]
    pub fn draw_elements(
        &mut self,
        canvas: &mut PixmapMut,
        elements: &[Element],
        assets: &mut AssetStore,
        t: i64,
        offset: (f32, f32),
    ) {
        for el in elements {
            match el {
                Element::Rect { rect, fill, common } => {
                    let resolved = resolve_common(common, t);
                    let bbox = BBox {
                        x: rect[0].0 as f32 + offset.0,
                        y: rect[1].0 as f32 + offset.1,
                        w: rect[2].0 as f32,
                        h: rect[3].0 as f32,
                    };
                    let matrix = element_matrix(&bbox, &resolved);

                    let Some(sk_rect) = SkRect::from_xywh(bbox.x, bbox.y, bbox.w, bbox.h) else {
                        continue; // degenerate (zero/negative size) rect: nothing to draw
                    };
                    let path = PathBuilder::from_rect(sk_rect);

                    let (r, g, b, a) = fill.rgba8();
                    let alpha = ((a as f32) * (resolved.opacity as f32))
                        .round()
                        .clamp(0.0, 255.0) as u8;
                    let paint = Paint {
                        shader: Shader::SolidColor(tiny_skia::Color::from_rgba8(r, g, b, alpha)),
                        anti_alias: true,
                        ..Default::default()
                    };

                    canvas.fill_path(&path, &paint, FillRule::Winding, matrix, None);
                }
                Element::Image {
                    asset,
                    rect,
                    common,
                } => {
                    let resolved = resolve_common(common, t);
                    let bbox = BBox {
                        x: rect[0].0 as f32 + offset.0,
                        y: rect[1].0 as f32 + offset.1,
                        w: rect[2].0 as f32,
                        h: rect[3].0 as f32,
                    };

                    let src = assets.image(asset);
                    let matrix = element_matrix(&bbox, &resolved)
                        .pre_translate(bbox.x, bbox.y)
                        .pre_scale(bbox.w / src.width() as f32, bbox.h / src.height() as f32);

                    let paint = PixmapPaint {
                        opacity: resolved.opacity as f32,
                        blend_mode: BlendMode::SourceOver,
                        quality: FilterQuality::Bilinear,
                    };

                    canvas.draw_pixmap(0, 0, src.as_ref(), &paint, matrix, None);
                }
                Element::Text { .. } => {
                    // implemented in Task 10
                }
                Element::Group { .. } => {
                    // implemented in Task 11
                }
            }
        }
    }
}
