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
use crate::doc::{Align, Cap, Element, Gradient, Join, Paint};
use crate::text::{layout_text, TextLayout};
use cosmic_text::SwashContent;
use std::collections::HashMap;
use tiny_skia::{
    BlendMode, Color, FillRule, FilterQuality, GradientStop, LineCap, LineJoin, LinearGradient,
    Mask, Paint as SkPaint, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Point, RadialGradient,
    Rect as SkRect, Shader, SpreadMode, Stroke, Transform,
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
        Element::Path { points, .. } => {
            let mut it = points.iter();
            let Some(first) = it.next() else {
                return BBox {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0,
                };
            };
            let (mut x0, mut y0) = (first[0].0 as f32, first[1].0 as f32);
            let (mut x1, mut y1) = (x0, y0);
            for p in it {
                let (px, py) = (p[0].0 as f32, p[1].0 as f32);
                x0 = x0.min(px);
                y0 = y0.min(py);
                x1 = x1.max(px);
                y1 = y1.max(py);
            }
            BBox {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            }
        }
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

/// Build the tiny-skia shader for a `Paint` over `bbox`.
///
/// Gradient coordinates are unit-space over the element's own box, mapped
/// here into the same local space as the geometry being filled. `fill_path`
/// applies its transform to the shader as well as the path (see tiny-skia's
/// `painter.rs`), so a rotated element carries its gradient with it without
/// any extra work, and without the matrix being applied twice.
///
/// Alpha is folded into every stop rather than applied afterwards, so a
/// gradient honours an animated `opacity` exactly as a solid fill does.
fn shader_for(paint: &Paint, bbox: &BBox, opacity: f64) -> Shader<'static> {
    let scale = |c: &crate::color::Color| {
        let (r, g, b, a) = c.rgba8();
        let alpha = ((a as f32) * (opacity as f32)).round().clamp(0.0, 255.0) as u8;
        Color::from_rgba8(r, g, b, alpha)
    };

    let gradient = match paint {
        Paint::Solid(c) => return Shader::SolidColor(scale(c)),
        Paint::Gradient(g) => g,
    };

    let stops: Vec<GradientStop> = gradient
        .stops()
        .iter()
        .map(|s| GradientStop::new(s.at.0 as f32, scale(&s.color)))
        .collect();

    // Unit space to the element's own box.
    let at =
        |u: f64, v: f64| Point::from_xy(bbox.x + (u as f32) * bbox.w, bbox.y + (v as f32) * bbox.h);

    let built = match gradient {
        Gradient::Linear { from, to, .. } => LinearGradient::new(
            at(from[0].0, from[1].0),
            at(to[0].0, to[1].0),
            stops,
            SpreadMode::Pad,
            Transform::identity(),
        ),
        Gradient::Radial { center, radius, .. } => {
            // Radius is a fraction of the longer edge, so a wide box does not
            // get an ellipse-shaped falloff on one axis only.
            let r = (radius.0 as f32) * bbox.w.abs().max(bbox.h.abs());
            let c = at(center[0].0, center[1].0);
            RadialGradient::new(
                c,
                0.0,
                c,
                r.max(f32::EPSILON),
                stops,
                SpreadMode::Pad,
                Transform::identity(),
            )
        }
    };

    // `None` only for a degenerate gradient (validation rejects those), so
    // falling back to the first stop keeps the renderer total.
    built.unwrap_or_else(|| {
        Shader::SolidColor(
            gradient
                .stops()
                .first()
                .map(|s| scale(&s.color))
                .unwrap_or(Color::TRANSPARENT),
        )
    })
}

/// Build a full-canvas mask from a clip window, in the element's parent
/// space.
///
/// Deliberately not passed through `element_matrix`: a clip that travelled
/// with its content could never reveal anything. The window stays put and the
/// content animates behind it.
fn clip_mask(w: u32, h: u32, clip: &crate::doc::Clip, offset: (f32, f32)) -> Option<Mask> {
    let rect = SkRect::from_xywh(
        clip.rect[0].0 as f32 + offset.0,
        clip.rect[1].0 as f32 + offset.1,
        clip.rect[2].0 as f32,
        clip.rect[3].0 as f32,
    )?;
    let path = rounded_rect(rect, clip.radius.map(|r| r.0 as f32).unwrap_or(0.0))?;
    let mut mask = Mask::new(w, h)?;
    mask.fill_path(&path, FillRule::Winding, true, Transform::identity());
    Some(mask)
}

/// Separable box blur over a mask's coverage bytes, three passes.
///
/// Three box passes approximate a Gaussian closely enough that the difference
/// is invisible, and the whole thing is integer arithmetic — running sums in
/// `u32`, one division per pixel. That matters more than speed here: no
/// floating point means no new surface for native and wasm to disagree on,
/// which is why a shadow could be added without re-opening the parity
/// question the way a float filter would have.
fn box_blur(data: &mut [u8], w: usize, h: usize, radius: usize) {
    if radius == 0 || w == 0 || h == 0 {
        return;
    }
    let mut scratch = vec![0u8; data.len()];
    for _ in 0..3 {
        blur_pass(data, &mut scratch, w, h, radius, true);
        blur_pass(&scratch, data, w, h, radius, false);
    }
}

fn blur_pass(src: &[u8], dst: &mut [u8], w: usize, h: usize, radius: usize, horizontal: bool) {
    let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
    let window = (radius * 2 + 1) as u32;
    for o in 0..outer {
        let at = |i: usize| if horizontal { o * w + i } else { i * w + o };
        // Seed the running sum with the clamped left half of the window.
        let mut sum: u32 = 0;
        for k in 0..=radius.min(inner - 1) {
            sum += src[at(k)] as u32;
        }
        sum += src[at(0)] as u32 * radius.min(inner) as u32;
        for i in 0..inner {
            dst[at(i)] = (sum / window) as u8;
            let add = src[at((i + radius + 1).min(inner - 1))] as u32;
            let sub = src[at(i.saturating_sub(radius))] as u32;
            sum = sum + add - sub;
        }
    }
}

/// Draw `path`'s silhouette, offset and blurred, beneath the element.
#[allow(clippy::too_many_arguments)]
fn draw_shadow(
    canvas: &mut PixmapMut,
    path: &tiny_skia::Path,
    shadow: &crate::doc::Shadow,
    matrix: Transform,
    opacity: f64,
    clip: Option<&Mask>,
) {
    let (w, h) = (canvas.width(), canvas.height());
    let Some(mut mask) = Mask::new(w, h) else {
        return;
    };
    let offset = Transform::from_translate(shadow.dx.0 as f32, shadow.dy.0 as f32);
    mask.fill_path(path, FillRule::Winding, true, matrix.post_concat(offset));
    box_blur(
        mask.data_mut(),
        w as usize,
        h as usize,
        shadow.blur as usize,
    );

    let (r, g, b, a) = shadow.color.rgba8();
    let alpha = ((a as f32) * (opacity as f32)).round().clamp(0.0, 255.0) as u8;
    let paint = SkPaint {
        shader: Shader::SolidColor(Color::from_rgba8(r, g, b, alpha)),
        anti_alias: true,
        ..Default::default()
    };
    // The blurred silhouette is the mask; the fill is a full-canvas rect
    // through it. An author-supplied clip still applies, so a shadow cannot
    // escape a window its element was confined to.
    let Some(full) = SkRect::from_xywh(0.0, 0.0, w as f32, h as f32) else {
        return;
    };
    let cover = PathBuilder::from_rect(full);
    if let Some(user) = clip {
        let mut combined = mask.clone();
        for (m, u) in combined.data_mut().iter_mut().zip(user.data()) {
            *m = ((*m as u32 * *u as u32) / 255) as u8;
        }
        canvas.fill_path(
            &cover,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            Some(&combined),
        );
    } else {
        canvas.fill_path(
            &cover,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            Some(&mask),
        );
    }
}

/// Kappa: the cubic control-point offset that approximates a quarter circle
/// to within about 0.02%. The standard constant, not a guess.
const KAPPA: f32 = 0.552_284_8;

/// A rectangle path, with rounded corners when `r > 0`.
///
/// The radius is clamped to half the shorter edge, so an over-large value
/// degrades to a stadium rather than folding the path inside out.
fn rounded_rect(rect: SkRect, r: f32) -> Option<tiny_skia::Path> {
    // Spelled out rather than `!(r > 0.0)`: NaN must fall through to a plain
    // rectangle too, and the negated comparison hid that.
    if r.is_nan() || r <= 0.0 {
        return Some(PathBuilder::from_rect(rect));
    }
    let (l, t, right, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    let r = r.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let c = r * KAPPA;

    let mut pb = PathBuilder::new();
    pb.move_to(l + r, t);
    pb.line_to(right - r, t);
    pb.cubic_to(right - r + c, t, right, t + r - c, right, t + r);
    pb.line_to(right, b - r);
    pb.cubic_to(right, b - r + c, right - r + c, b, right - r, b);
    pb.line_to(l + r, b);
    pb.cubic_to(l + r - c, b, l, b - r + c, l, b - r);
    pb.line_to(l, t + r);
    pb.cubic_to(l, t + r - c, l + r - c, t, l + r, t);
    pb.close();
    pb.finish()
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
                Element::Rect {
                    rect,
                    fill,
                    radius,
                    common,
                } => {
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
                    let r = radius.map(|r| r.0 as f32).unwrap_or(0.0);
                    let Some(path) = rounded_rect(sk_rect, r) else {
                        continue;
                    };

                    let paint = SkPaint {
                        shader: shader_for(fill, &bbox, resolved.opacity),
                        anti_alias: true,
                        ..Default::default()
                    };

                    let mask = common
                        .clip
                        .as_ref()
                        .and_then(|c| clip_mask(canvas.width(), canvas.height(), c, offset));
                    if let Some(sh) = &common.shadow {
                        draw_shadow(canvas, &path, sh, matrix, resolved.opacity, mask.as_ref());
                    }
                    canvas.fill_path(&path, &paint, FillRule::Winding, matrix, mask.as_ref());
                }
                Element::Path {
                    points,
                    closed,
                    stroke,
                    stroke_width,
                    cap,
                    join,
                    fill,
                    common,
                } => {
                    let resolved = resolve_common(common, t);
                    // Pivot off the point bounds, so `rotation`/`scale` turn a
                    // path about its own centre like every other element.
                    let base = base_bbox(el);
                    let bbox = BBox {
                        x: base.x + offset.0,
                        y: base.y + offset.1,
                        w: base.w,
                        h: base.h,
                    };
                    let matrix = element_matrix(&bbox, &resolved);

                    let mut pb = PathBuilder::new();
                    let mut pts = points.iter();
                    let Some(first) = pts.next() else {
                        continue; // validation rejects this; the renderer stays total
                    };
                    pb.move_to(first[0].0 as f32 + offset.0, first[1].0 as f32 + offset.1);
                    for p in pts {
                        pb.line_to(p[0].0 as f32 + offset.0, p[1].0 as f32 + offset.1);
                    }
                    if *closed {
                        pb.close();
                    }
                    let Some(path) = pb.finish() else {
                        continue; // degenerate (e.g. every point identical)
                    };

                    let paint_for = |p: &Paint| SkPaint {
                        shader: shader_for(p, &bbox, resolved.opacity),
                        anti_alias: true,
                        ..Default::default()
                    };
                    let stroke_paint =
                        |c: &crate::color::Color| paint_for(&Paint::Solid(c.clone()));

                    let mask = common
                        .clip
                        .as_ref()
                        .and_then(|c| clip_mask(canvas.width(), canvas.height(), c, offset));

                    if let Some(sh) = &common.shadow {
                        draw_shadow(canvas, &path, sh, matrix, resolved.opacity, mask.as_ref());
                    }

                    // Fill first so a stroke reads as an outline on top of it.
                    if let Some(f) = fill {
                        canvas.fill_path(
                            &path,
                            &paint_for(f),
                            FillRule::Winding,
                            matrix,
                            mask.as_ref(),
                        );
                    }
                    if let Some(s) = stroke {
                        let sk_stroke = Stroke {
                            // Absent width means 1.0, not 0.0 — a zero-width
                            // stroke paints nothing at all.
                            width: stroke_width.map(|w| w.0 as f32).unwrap_or(1.0),
                            line_cap: match cap {
                                Cap::Butt => LineCap::Butt,
                                Cap::Round => LineCap::Round,
                                Cap::Square => LineCap::Square,
                            },
                            line_join: match join {
                                Join::Miter => LineJoin::Miter,
                                Join::Round => LineJoin::Round,
                                Join::Bevel => LineJoin::Bevel,
                            },
                            ..Default::default()
                        };
                        canvas.stroke_path(
                            &path,
                            &stroke_paint(s),
                            &sk_stroke,
                            matrix,
                            mask.as_ref(),
                        );
                    }
                }
                Element::Image {
                    asset,
                    rect,
                    fit,
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
                    let (sw, sh) = (src.width() as f32, src.height() as f32);
                    let (sx, sy) = (bbox.w / sw, bbox.h / sh);
                    // Contain and cover keep the aspect ratio and centre the
                    // result; cover then needs the box as a clip, or it spills.
                    let (fx, fy) = match fit {
                        crate::doc::Fit::Stretch => (sx, sy),
                        crate::doc::Fit::Contain => {
                            let s = sx.min(sy);
                            (s, s)
                        }
                        crate::doc::Fit::Cover => {
                            let s = sx.max(sy);
                            (s, s)
                        }
                    };
                    let (dx, dy) = (
                        bbox.x + (bbox.w - sw * fx) / 2.0,
                        bbox.y + (bbox.h - sh * fy) / 2.0,
                    );
                    let matrix = element_matrix(&bbox, &resolved)
                        .pre_translate(dx, dy)
                        .pre_scale(fx, fy);

                    let paint = PixmapPaint {
                        opacity: resolved.opacity as f32,
                        blend_mode: BlendMode::SourceOver,
                        quality: FilterQuality::Bilinear,
                    };

                    // Cover crops to the element's own box; an explicit clip
                    // narrows it further. Built here rather than shared
                    // because cover's window is the element, not the author's.
                    let auto = if matches!(fit, crate::doc::Fit::Cover) {
                        Some(crate::doc::Clip {
                            rect: [
                                crate::Scalar((bbox.x - offset.0) as f64),
                                crate::Scalar((bbox.y - offset.1) as f64),
                                crate::Scalar(bbox.w as f64),
                                crate::Scalar(bbox.h as f64),
                            ],
                            radius: None,
                        })
                    } else {
                        None
                    };
                    let mask = common
                        .clip
                        .as_ref()
                        .or(auto.as_ref())
                        .and_then(|c| clip_mask(canvas.width(), canvas.height(), c, offset));
                    if let Some(sh) = &common.shadow {
                        // An image's silhouette is its own box.
                        if let Some(r) = SkRect::from_xywh(bbox.x, bbox.y, bbox.w, bbox.h) {
                            let sil = PathBuilder::from_rect(r);
                            let m = element_matrix(&bbox, &resolved);
                            draw_shadow(canvas, &sil, sh, m, resolved.opacity, mask.as_ref());
                        }
                    }
                    canvas.draw_pixmap(0, 0, src.as_ref(), &paint, matrix, mask.as_ref());
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
