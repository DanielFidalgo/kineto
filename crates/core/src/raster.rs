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
use crate::doc::{Align, Element};
use crate::text::{layout_text, TextLayout};
use cosmic_text::SwashContent;
use std::collections::HashMap;
use tiny_skia::{
    BlendMode, Color, FillRule, FilterQuality, Paint, PathBuilder, Pixmap, PixmapMut, PixmapPaint,
    Rect as SkRect, Shader, Transform,
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
/// Text's box is a placeholder — `pos` with zero size. Task 10 wires the
/// real `layout_text` box into `Renderer::draw_elements`'s `Text` arm
/// directly (it has `FontSystem` access there), but deliberately leaves
/// *this* placeholder as-is: `base_bbox` is a free function with no
/// `FontSystem`, so giving it a real text box would mean threading font
/// access through every `base_bbox` caller (currently only group-union
/// math) for a case — text nested in a `Group` — that doesn't exist yet.
/// Revisit when a text-in-group case needs an accurate group union box.
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

/// Everything `layout_text` is a pure function of (spec §5) — so two Text
/// elements with an equal key are guaranteed to lay out identically, and one
/// shaping pass can be reused for the other. The `f32` inputs are stored as
/// their raw `to_bits()` patterns: `f32` is not `Eq`/`Hash`, and a bit-exact
/// key is exactly the right notion of equality here anyway (two sizes that
/// compare `==` but differ in bits cannot exist for finite values, and NaN —
/// which `==` would reject — is rejected by validation long before this).
#[derive(Clone, PartialEq, Eq, Hash)]
struct LayoutKey {
    family: String,
    text: String,
    size_px_bits: u32,
    max_w_bits: Option<u32>,
    align: Align,
}

/// Renders elements to pixels. Owns three cross-frame caches, all of which
/// are pure memoization — they change how long a frame takes, never which
/// bytes it produces (the golden corpus and the native/wasm parity gate are
/// the proof; see `tests/golden.rs` and `tests/parity/run.mjs`):
///
/// - `swash`: the glyph raster cache (Task 10).
/// - `layouts`: shaped text layouts (Task 27). Text is re-shaped from
///   scratch on every frame otherwise, even though a v1 document's text is
///   static (spec §3.3: only translate/scale/rotation/opacity animate, and
///   none of those feed `layout_text`) — so per-run shaping is pure waste.
///   The cache is deliberately unbounded and never evicted: a document has a
///   finite, small set of distinct (family, text, size, wrap, align) tuples
///   (one per Text element at most, and the tape demo's captions repeat
///   across scenes), and an `Engine` — which owns exactly one `Renderer` —
///   lives only as long as the document it renders.
/// - `layer_pool`: scratch full-canvas pixmaps for the isolated layers the
///   Text and Group arms composite through (Task 27). Without it every such
///   element allocates and frees a full canvas (4 MB at 1280×800) on every
///   frame. Retained memory is bounded by the *concurrent* layer high-water
///   mark — i.e. the document's deepest Group nesting, +1 — not by the
///   number of Text/Group elements: siblings hand the same pixmap back and
///   forth, so a flat scene of N text elements keeps exactly one.
///
/// None of this is where the tape demo's frame time goes, measured (Task 27
/// report): full-canvas `draw_pixmap` compositing is ~95% of it. These two
/// caches are the cheap, semantics-preserving levers spec §8 names; the
/// expensive lever (ink-bbox-sized layers instead of full-canvas ones) is a
/// separate change with its own byte-identity argument to make.
pub struct Renderer {
    pub swash: cosmic_text::SwashCache,
    layouts: HashMap<LayoutKey, TextLayout>,
    layer_pool: Vec<Pixmap>,
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
            layouts: HashMap::new(),
            layer_pool: Vec::new(),
        }
    }

    /// Take a transparent `w`×`h` scratch layer from the pool, or allocate
    /// one if the pool is empty (or — canvas size is fixed for a given
    /// `Engine`, so this should not happen in practice — holds a pixmap of
    /// the wrong size, in which case the stale one is simply dropped).
    ///
    /// Byte-identity: a pooled pixmap is cleared with `Color::TRANSPARENT`,
    /// whose premultiplied form is all-zero — i.e. exactly the state
    /// `Pixmap::new` hands back — so a reused layer is indistinguishable
    /// from a fresh one. `Engine::render` already relies on this same
    /// property for its crossfade `scratch` buffer.
    fn acquire_layer(&mut self, w: u32, h: u32) -> Pixmap {
        while let Some(mut p) = self.layer_pool.pop() {
            if p.width() == w && p.height() == h {
                p.fill(Color::TRANSPARENT);
                return p;
            }
        }
        Pixmap::new(w, h).expect("canvas dimensions are always non-zero")
    }

    /// Return a layer to the pool. Pop/push (stack) discipline is what makes
    /// this recursion-safe for nested groups: a layer is only released after
    /// the recursive `draw_elements` that drew into it has returned, so a
    /// nested scope can never be handed the layer its parent is still
    /// drawing into.
    fn release_layer(&mut self, layer: Pixmap) {
        self.layer_pool.push(layer);
    }

    /// Shaped layout for one Text element, from `layouts` on a hit and from
    /// `layout_text` (inserting) on a miss.
    ///
    /// Returns an owned clone rather than a borrow on purpose: the caller
    /// needs `&mut self.swash` for glyph rasterization immediately after,
    /// and a `&TextLayout` borrowed out of `self.layouts` would keep `self`
    /// borrowed across that. The clone is a `Vec<PlacedGlyph>` memcpy of a
    /// few hundred bytes — orders of magnitude below the shaping pass it
    /// replaces, and below the per-frame pixmap work around it.
    fn layout_for(
        &mut self,
        assets: &mut AssetStore,
        family: String,
        text: &str,
        size_px: f32,
        max_w: Option<f32>,
        align: Align,
    ) -> TextLayout {
        let key = LayoutKey {
            family,
            text: text.to_string(),
            size_px_bits: size_px.to_bits(),
            max_w_bits: max_w.map(f32::to_bits),
            align,
        };
        if let Some(hit) = self.layouts.get(&key) {
            return hit.clone();
        }
        let layout = layout_text(
            assets.font_system(),
            &key.family,
            text,
            size_px,
            max_w,
            align,
        );
        self.layouts.insert(key, layout.clone());
        layout
    }

    /// Draw `elements` (already in document/paint order, spec §3.3) onto
    /// `canvas` at time `t`, with `offset` the accumulated parent-group
    /// origin (scene root passes `(0.0, 0.0)`).
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
                Element::Text {
                    text,
                    font,
                    size_px,
                    color,
                    pos,
                    max_w,
                    align,
                    common,
                } => {
                    let resolved = resolve_common(common, t);

                    // `assets.family(font)` borrows `assets` immutably; copy
                    // it to an owned `String` before `assets.font_system()`
                    // needs `assets` mutably for `layout_text` below (the
                    // two borrows can't overlap otherwise). The owned
                    // `String` doubles as part of the layout cache key.
                    let family = assets.family(font).to_string();
                    let layout = self.layout_for(
                        assets,
                        family,
                        text,
                        size_px.0 as f32,
                        max_w.as_ref().map(|w| w.0 as f32),
                        *align,
                    );

                    let pos_x = pos[0].0 as f32 + offset.0;
                    let pos_y = pos[1].0 as f32 + offset.1;

                    // Isolated full-canvas transparent layer: uniform with
                    // how groups will composite (Task 11), and it makes the
                    // element's `translate/scale/rotation` transform (below)
                    // trivial to apply as a single `draw_pixmap` call instead
                    // of transforming every glyph individually.
                    //
                    // LIMITATION: glyphs are blitted into this layer at
                    // their *static* pixel position (`pos` + offset, before
                    // `element_matrix` is applied) and clipped to the
                    // canvas bounds at that point — see the bounds checks in
                    // `blit_mask`/`blit_color_premul`. So ink that falls
                    // outside the canvas at the untransformed `pos` is lost
                    // even if `translate`/`scale`/`rotation` would have
                    // brought it back into frame; only ink that's already
                    // on-canvas pre-transform survives to be composited.
                    // This is a real correctness gap, not just a
                    // convenience trade-off — Task 11 (groups, which
                    // composite the same way) will carry the same note
                    // until a canvas-sized-plus-margin (or unbounded) layer
                    // is worth the cost.
                    let mut layer = self.acquire_layer(canvas.width(), canvas.height());
                    let (cr, cg, cb, ca) = color.rgba8();

                    // `self.swash` (rasterizes) and `assets.font_system()`
                    // (glyph source) are different objects, so this mutable
                    // borrow of `assets` doesn't conflict with anything
                    // `layer`/`self.swash` hold.
                    let fs = assets.font_system();
                    for g in &layout.glyphs {
                        let Some(img) = self.swash.get_image_uncached(fs, g.cache_key) else {
                            continue; // no bitmap for this glyph (e.g. whitespace)
                        };
                        if img.placement.width == 0 || img.placement.height == 0 {
                            continue;
                        }
                        // placement.top is SUBTRACTED: swash's placement
                        // origin is the glyph's top-left in a y-up-from-
                        // baseline space, so flipping to our y-down pixel
                        // space negates the vertical offset.
                        let x0 = pos_x.round() as i32 + g.x + img.placement.left;
                        let y0 = pos_y.round() as i32 + g.y - img.placement.top;
                        match img.content {
                            SwashContent::Mask | SwashContent::SubpixelMask => blit_mask(
                                &mut layer,
                                x0,
                                y0,
                                img.placement.width,
                                img.placement.height,
                                &img.data,
                                (cr, cg, cb, ca),
                            ),
                            SwashContent::Color => blit_color_premul(
                                &mut layer,
                                x0,
                                y0,
                                img.placement.width,
                                img.placement.height,
                                &img.data,
                            ),
                        }
                    }

                    // The pivot/transform box is the *alignment* box, not
                    // the ink box: cosmic-text positions glyphs (via `align`)
                    // within a box of width `max_w` when it's set, so
                    // Center/Right-aligned glyphs can sit well right of
                    // `layout.width` (the max *line* width, i.e. the ink
                    // extent) — using ink width here would pivot rotation/
                    // scale off-center from what was actually laid out.
                    // Falls back to `layout.width` when `max_w` is unset
                    // (single line, no alignment box to speak of).
                    let bbox_w = max_w.as_ref().map(|w| w.0 as f32).unwrap_or(layout.width);
                    let text_bbox = BBox {
                        x: pos_x,
                        y: pos_y,
                        w: bbox_w,
                        h: layout.height,
                    };
                    let matrix = element_matrix(&text_bbox, &resolved);
                    let paint = PixmapPaint {
                        opacity: resolved.opacity as f32,
                        blend_mode: BlendMode::SourceOver,
                        quality: FilterQuality::Bilinear,
                    };
                    canvas.draw_pixmap(0, 0, layer.as_ref(), &paint, matrix, None);
                    self.release_layer(layer);
                }
                Element::Group {
                    origin,
                    children,
                    common,
                } => {
                    let resolved = resolve_common(common, t);

                    // Group's own box is the static union of its children's
                    // boxes (Task 8's `base_bbox`, which already bakes in
                    // this group's `origin`), shifted by the accumulated
                    // parent offset — same convention as every other arm's
                    // `bbox` above.
                    let b = base_bbox(el);
                    let group_bbox = BBox {
                        x: b.x + offset.0,
                        y: b.y + offset.1,
                        w: b.w,
                        h: b.h,
                    };
                    let matrix = element_matrix(&group_bbox, &resolved);

                    // Isolated full-canvas transparent layer, same pattern
                    // as the Text arm above (Task 10) and for the same
                    // reason: this element's own translate/scale/rotation/
                    // opacity apply once to the whole composited subtree,
                    // as a single `draw_pixmap` call, not per child.
                    //
                    // Critically: group opacity MUST NOT be
                    // pushed into children (spec §3.3 isolation semantics)
                    // — recursing with `resolved.opacity` baked into the
                    // children's own resolution would double-fade any
                    // overlap between them, which is exactly what
                    // isolated compositing exists to avoid. So the
                    // recursive `draw_elements` call below draws children
                    // onto the transparent layer at their own opacity
                    // only; `resolved.opacity` is applied exactly once,
                    // below, via the layer's own composite `PixmapPaint`.
                    //
                    // LIMITATION: same pre-clip caveat as the Text arm's
                    // comment above — children are composited into this
                    // layer at their *static* (untransformed-by-this-
                    // group) positions and clipped to the canvas bounds at
                    // that point, so ink that falls outside the canvas
                    // before this group's own transform is applied is
                    // lost even if the transform would bring it back into
                    // frame.
                    let mut layer = self.acquire_layer(canvas.width(), canvas.height());
                    self.draw_elements(
                        &mut layer.as_mut(),
                        children,
                        assets,
                        t,
                        (offset.0 + origin[0].0 as f32, offset.1 + origin[1].0 as f32),
                    );

                    let paint = PixmapPaint {
                        opacity: resolved.opacity as f32,
                        blend_mode: BlendMode::SourceOver,
                        quality: FilterQuality::Bilinear,
                    };
                    canvas.draw_pixmap(0, 0, layer.as_ref(), &paint, matrix, None);
                    self.release_layer(layer);
                }
            }
        }
    }
}

/// Rounding "divide by 255", the same formula as `assets.rs`'s
/// `premultiply_rgba` (tiny-skia's own rounding-division approximation:
/// `((x + 128) + ((x + 128) >> 8)) >> 8`). Used here for both the alpha-mask
/// tint/premultiply step and the per-pixel source-over compose, so glyph
/// blitting is pure integer math — no floats, bit-identical on native and
/// wasm (determinism is law, spec §5).
#[inline]
fn div255(x: u32) -> u32 {
    let x = x + 128;
    (x + (x >> 8)) >> 8
}

/// Source-over composite one premultiplied `src` pixel onto `dst` (also
/// premultiplied), returning the premultiplied result. Shared by both blit
/// paths below; each channel of the output is `<= out_a` by construction
/// (see `div255`'s monotonicity), but components are still clamped to the
/// resulting alpha as a defensive belt-and-braces check before handing the
/// bytes to `tiny_skia::PremultipliedColorU8::from_rgba`, which rejects
/// (returns `None`) anything that violates that invariant.
#[inline]
fn over_premul(dst: (u8, u8, u8, u8), src: (u8, u8, u8, u8)) -> tiny_skia::PremultipliedColorU8 {
    let inv = 255 - src.3 as u32;
    let out_a = src.3 as u32 + div255(dst.3 as u32 * inv);
    let out_r = (src.0 as u32 + div255(dst.0 as u32 * inv)).min(out_a);
    let out_g = (src.1 as u32 + div255(dst.1 as u32 * inv)).min(out_a);
    let out_b = (src.2 as u32 + div255(dst.2 as u32 * inv)).min(out_a);
    tiny_skia::PremultipliedColorU8::from_rgba(out_r as u8, out_g as u8, out_b as u8, out_a as u8)
        .expect("channels clamped to <= alpha above")
}

/// Blit an 8-bit alpha mask glyph bitmap (`SwashContent::Mask` /
/// `SubpixelMask`, treated identically per the task brief) into `layer` at
/// `(x0, y0)`, tinted by the element's straight-alpha text `color` and
/// source-over composited (glyphs can have overlapping antialiased edges,
/// so this is a real compose, not an overwrite).
#[allow(clippy::too_many_arguments)]
fn blit_mask(
    layer: &mut Pixmap,
    x0: i32,
    y0: i32,
    w: u32,
    h: u32,
    mask: &[u8],
    color: (u8, u8, u8, u8),
) {
    let (cw, ch) = (layer.width() as i32, layer.height() as i32);
    let (cr, cg, cb, ca) = color;
    let pixels = layer.pixels_mut();
    for row in 0..h as i32 {
        let py = y0 + row;
        if py < 0 || py >= ch {
            continue;
        }
        for col in 0..w as i32 {
            let px = x0 + col;
            if px < 0 || px >= cw {
                continue;
            }
            let m = mask[(row as usize) * (w as usize) + col as usize] as u32;
            let src_a = div255(m * ca as u32);
            if src_a == 0 {
                continue;
            }
            let src_r = div255(cr as u32 * src_a) as u8;
            let src_g = div255(cg as u32 * src_a) as u8;
            let src_b = div255(cb as u32 * src_a) as u8;
            let idx = (py as usize) * (cw as usize) + px as usize;
            pixels[idx] = over_premul(
                (
                    pixels[idx].red(),
                    pixels[idx].green(),
                    pixels[idx].blue(),
                    pixels[idx].alpha(),
                ),
                (src_r, src_g, src_b, src_a as u8),
            );
        }
    }
}

/// Blit a premultiplied-RGBA color glyph bitmap (`SwashContent::Color` —
/// emoji-style fonts, spec §8) into `layer` at `(x0, y0)`, source-over
/// composited (no color tint: the bitmap's own colors are used as-is).
fn blit_color_premul(layer: &mut Pixmap, x0: i32, y0: i32, w: u32, h: u32, rgba: &[u8]) {
    let (cw, ch) = (layer.width() as i32, layer.height() as i32);
    let pixels = layer.pixels_mut();
    for row in 0..h as i32 {
        let py = y0 + row;
        if py < 0 || py >= ch {
            continue;
        }
        for col in 0..w as i32 {
            let px = x0 + col;
            if px < 0 || px >= cw {
                continue;
            }
            let src_idx = ((row as usize) * (w as usize) + col as usize) * 4;
            let src = (
                rgba[src_idx],
                rgba[src_idx + 1],
                rgba[src_idx + 2],
                rgba[src_idx + 3],
            );
            if src.3 == 0 {
                continue;
            }
            let idx = (py as usize) * (cw as usize) + px as usize;
            pixels[idx] = over_premul(
                (
                    pixels[idx].red(),
                    pixels[idx].green(),
                    pixels[idx].blue(),
                    pixels[idx].alpha(),
                ),
                src,
            );
        }
    }
}
