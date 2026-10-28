//! Golden corpus (spec §6 / task 14): documents built entirely with the
//! Rust builders (`doc.rs`), referencing only `testdata/assets/*` images and
//! the reserved font srcs (`assets.rs::resolve_reserved_src`).
//!
//! This corpus is the **parity gate's substrate**: Task 16 renders exactly
//! these documents in wasm and diffs pixels against the native hashes this
//! module's consumer (`tests/golden.rs`) pins. So `corpus()` itself must
//! compile on wasm32 (no `std::fs`, no native-only APIs) — only
//! `corpus_load_assets` (which does touch the filesystem) is native-gated;
//! `crates/wasm` re-exports this module behind its own feature and supplies
//! asset bytes a different way.

use crate::doc::{
    ms, Align, Asset, Cap, Clip, Document, Ease, Element, Fit, Gradient, Join, Key, Prop, Scene,
    Shadow, Stop, Track, Transition,
};

#[cfg(all(not(target_arch = "wasm32"), feature = "bundled-fonts"))]
use crate::assets::{resolve_reserved_src, AssetStore};

/// One corpus document plus the ticks it should be sampled at.
pub struct CorpusDoc {
    pub name: &'static str,
    pub doc: Document,
    pub ticks: Vec<i64>,
}

/// The full golden corpus (spec §6 coverage: every element type, every
/// easing, crossfade, wrap, group nesting).
pub fn corpus() -> Vec<CorpusDoc> {
    vec![
        rects_easings(),
        paths_strokes(),
        curves(),
        curves_edge(),
        curves_collinear(),
        draw_on(),
        shadow_stroke(),
        gradients(),
        radius_easings(),
        clip_fit(),
        shadows(),
        image_transform(),
        text_wrap(),
        groups_nested(),
        crossfade_doc(),
        kitchen_sink(),
    ]
}

const W: u32 = 320;
const H: u32 = 200;

/// Four rects, one animated track each — every `Prop` paired with every
/// `Ease`: translate/linear, scale/inCubic, rotation/outCubic,
/// opacity/inOutCubic.
fn rects_easings() -> CorpusDoc {
    let dur = ms(1000);
    let mut doc = Document::new(W, H);
    let scene = Scene::new("s", dur)
        .with_element(
            Element::rect([10.0, 10.0, 60.0, 60.0], "#FF4040").with_animation(Track::new(
                Prop::Translate,
                vec![
                    Key::vec2(0, [0.0, 0.0]),
                    Key::vec2(dur, [80.0, 0.0]).with_ease(Ease::Linear),
                ],
            )),
        )
        .with_element(
            Element::rect([170.0, 10.0, 60.0, 60.0], "#40FF40").with_animation(Track::new(
                Prop::Scale,
                vec![
                    Key::num(0, 1.0),
                    Key::num(dur, 2.0).with_ease(Ease::InCubic),
                ],
            )),
        )
        .with_element(
            Element::rect([10.0, 110.0, 60.0, 60.0], "#4040FF").with_animation(Track::new(
                Prop::Rotation,
                vec![
                    Key::num(0, 0.0),
                    Key::num(dur, 90.0).with_ease(Ease::OutCubic),
                ],
            )),
        )
        .with_element(
            Element::rect([170.0, 110.0, 60.0, 60.0], "#FFFF40").with_animation(Track::new(
                Prop::Opacity,
                vec![
                    Key::num(0, 0.2),
                    Key::num(dur, 1.0).with_ease(Ease::InOutCubic),
                ],
            )),
        );
    doc.push_scene(scene);
    CorpusDoc {
        name: "rects-easings",
        doc,
        ticks: vec![0, dur / 2, dur - 1],
    }
}

/// Drop shadows: blurred, offset, and on all three element kinds that have a
/// silhouette. The blur is integer arithmetic, so this adds no new
/// floating-point surface for native and wasm to disagree over — the mask
/// rasterisation it rides on is already covered by `clip-fit`.
fn shadows() -> CorpusDoc {
    let dur = ms(600);
    let mut doc = Document::new(W, H).with_bg("#F2F5F7");
    doc.add_asset("grad", Asset::image("grad.png"));
    let scene = Scene::new("s", dur)
        .with_element(
            Element::rect([20.0, 20.0, 110.0, 60.0], "#FFFFFF")
                .with_radius(12.0)
                .with_shadow(Shadow::new("#0D141966", 10, 0.0, 6.0)),
        )
        // Hard silhouette: blur 0 is an offset shape, not a no-op.
        .with_element(
            Element::rect([170.0, 20.0, 110.0, 60.0], "#FF9900")
                .with_shadow(Shadow::new("#0D1419", 0, 8.0, 8.0)),
        )
        // On a closed path, and on an image.
        .with_element(
            Element::path(vec![[40.0, 180.0], [100.0, 110.0], [160.0, 180.0]])
                .with_closed(true)
                .with_path_fill("#C77DFF")
                .with_shadow(Shadow::new("#0D141980", 8, 4.0, 6.0)),
        )
        .with_element(
            Element::image("grad", [200.0, 110.0, 90.0, 70.0])
                .with_fit(Fit::Cover)
                .with_shadow(Shadow::new("#0D141999", 12, 0.0, 8.0)),
        );
    doc.push_scene(scene);
    CorpusDoc {
        name: "shadows",
        doc,
        ticks: vec![0],
    }
}

/// Clip windows and the three image fit modes.
///
/// grad.png rather than photo.jpg, deliberately: photo.jpg is a solid
/// colour, and for a solid image `cover` and `stretch` fill the box with
/// identical pixels — the golden could not tell them apart. A gradient makes
/// the crop visible. The clipped rect proves a window stays put while its
/// content slides behind it, which is the only way this format can express a
/// reveal.
fn clip_fit() -> CorpusDoc {
    let dur = ms(600);
    let mut doc = Document::new(W, H);
    doc.add_asset("grad", Asset::image("grad.png"));
    let scene = Scene::new("s", dur)
        .with_element(Element::image("grad", [10.0, 10.0, 90.0, 50.0]))
        .with_element(Element::image("grad", [110.0, 10.0, 90.0, 50.0]).with_fit(Fit::Contain))
        .with_element(Element::image("grad", [210.0, 10.0, 90.0, 50.0]).with_fit(Fit::Cover))
        // A rounded window over an image: a radius the image knows nothing of.
        .with_element(
            Element::image("grad", [10.0, 80.0, 80.0, 80.0])
                .with_fit(Fit::Cover)
                .with_clip(Clip::new([10.0, 80.0, 80.0, 80.0]).with_radius(24.0)),
        )
        // Content sliding behind a fixed window: at dur/2 it is halfway
        // through, so the window is partly filled.
        .with_element(
            Element::rect([110.0, 90.0, 190.0, 60.0], "#FF9900")
                .with_clip(Clip::new([110.0, 90.0, 190.0, 60.0]))
                .with_animation(Track::new(
                    Prop::Translate,
                    vec![
                        Key::vec2(0, [-190.0, 0.0]),
                        Key::vec2(dur, [0.0, 0.0]).with_ease(Ease::OutCubic),
                    ],
                )),
        );
    doc.push_scene(scene);
    CorpusDoc {
        name: "clip-fit",
        doc,
        ticks: vec![0, dur / 2],
    }
}

/// Rounded corners and the expressive easings, which only exist together
/// here: `back` overshoots past its target and `expo` is nearly flat for most
/// of its run, so both need a mid-tick sample to be worth anything.
fn radius_easings() -> CorpusDoc {
    let dur = ms(600);
    let mut doc = Document::new(W, H);
    let slide = |e: Ease, y: f64| {
        Element::rect([20.0, y, 90.0, 26.0], "#FF9900")
            .with_radius(13.0)
            .with_animation(Track::new(
                Prop::Translate,
                vec![
                    Key::vec2(0, [0.0, 0.0]),
                    Key::vec2(dur, [190.0, 0.0]).with_ease(e),
                ],
            ))
    };
    let scene = Scene::new("s", dur)
        .with_element(slide(Ease::OutBack, 12.0))
        .with_element(slide(Ease::InOutBack, 52.0))
        .with_element(slide(Ease::OutExpo, 92.0))
        .with_element(slide(Ease::InOutExpo, 132.0))
        // A radius larger than half the shorter edge, clamped to a stadium.
        .with_element(Element::rect([20.0, 172.0, 280.0, 20.0], "#4ECDC4").with_radius(999.0));
    doc.push_scene(scene);
    CorpusDoc {
        name: "radius-easings",
        doc,
        ticks: vec![0, dur / 2],
    }
}

/// Linear and radial gradients, including one carried through a rotation.
///
/// Gradient stops interpolate in premultiplied space and the shader is
/// transformed alongside the geometry, so this is the only corpus entry where
/// a rotation changes *how a fill is sampled* rather than only where it lands.
fn gradients() -> CorpusDoc {
    let dur = ms(600);
    let mut doc = Document::new(W, H);
    let scene = Scene::new("s", dur)
        .with_element(Element::rect(
            [10.0, 10.0, 140.0, 80.0],
            Gradient::linear(
                [0.0, 0.0],
                [1.0, 0.0],
                vec![Stop::new(0.0, "#FF9900"), Stop::new(1.0, "#4ECDC4")],
            ),
        ))
        // Three stops on a diagonal axis: exercises the middle-stop path.
        .with_element(Element::rect(
            [170.0, 10.0, 140.0, 80.0],
            Gradient::linear(
                [0.0, 0.0],
                [1.0, 1.0],
                vec![
                    Stop::new(0.0, "#C77DFF"),
                    Stop::new(0.35, "#F2F5F7"),
                    Stop::new(1.0, "#0D1419"),
                ],
            ),
        ))
        .with_element(Element::rect(
            [10.0, 110.0, 140.0, 80.0],
            Gradient::radial(
                [0.5, 0.5],
                0.6,
                vec![Stop::new(0.0, "#FFFFFF"), Stop::new(1.0, "#101820")],
            ),
        ))
        // Rotated: the shader turns with the geometry.
        .with_element(
            Element::rect(
                [170.0, 110.0, 140.0, 80.0],
                Gradient::linear(
                    [0.0, 0.0],
                    [1.0, 0.0],
                    vec![Stop::new(0.0, "#FF5C5C"), Stop::new(1.0, "#4ECDC4")],
                ),
            )
            .with_rotation(24.0),
        )
        // A gradient on a closed path, not only a rect.
        .with_element(
            Element::path(vec![[110.0, 96.0], [210.0, 96.0], [160.0, 104.0]])
                .with_closed(true)
                .with_path_fill(Gradient::linear(
                    [0.0, 0.0],
                    [1.0, 0.0],
                    vec![Stop::new(0.0, "#FF9900"), Stop::new(1.0, "#C77DFF")],
                )),
        );
    doc.push_scene(scene);
    CorpusDoc {
        name: "gradients",
        doc,
        ticks: vec![0, dur / 2],
    }
}

/// Stroked and filled polylines: the shapes whose antialiased coverage is
/// hardest to reproduce across targets.
///
/// Deliberately not a tidy diagram. Every existing corpus entry is
/// axis-aligned or glyph-based, so this is the only place the parity gate
/// exercises diagonal AA edges, miter joins, round joins built from
/// flattened conics, sub-pixel stroke widths, and a filled-and-stroked
/// closed shape. If native and wasm ever disagree about stroking, these
/// ticks are what will say so.
fn paths_strokes() -> CorpusDoc {
    let dur = ms(600);
    let mut doc = Document::new(W, H);
    let scene = Scene::new("s", dur)
        // Irrational-ish slope: the plainest non-axis-aligned AA edge.
        .with_element(Element::path(vec![[12.0, 18.0], [233.0, 97.0]]).with_stroke("#FF9900", 3.0))
        // Sharp corner with a long miter — the join most likely to expose a
        // floating-point difference.
        .with_element(
            Element::path(vec![[20.0, 150.0], [120.0, 104.0], [22.0, 100.0]])
                .with_stroke("#4ECDC4", 5.5)
                .with_cap(Cap::Square),
        )
        // Round join and cap: tiny-skia builds these from conics and
        // flattens them, so this is the highest-risk shape in the set.
        .with_element(
            Element::path(vec![[140.0, 160.0], [205.0, 112.0], [240.0, 172.0]])
                .with_stroke("#C77DFF", 7.25)
                .with_cap(Cap::Round)
                .with_join(Join::Round),
        )
        // Sub-pixel width: fractional coverage the whole length of the span.
        .with_element(
            Element::path(vec![[9.0, 185.0], [247.0, 179.0]]).with_stroke("#F2F5F7", 0.75),
        )
        // Filled and stroked closed shape — the arrowhead case — rotated so
        // the fill's edges are diagonal too.
        .with_element(
            Element::path(vec![[60.0, 30.0], [104.0, 52.0], [61.0, 74.0]])
                .with_closed(true)
                .with_path_fill("#FF9900")
                .with_stroke("#F2F5F7", 1.5)
                .with_rotation(12.0),
        );
    doc.push_scene(scene);
    CorpusDoc {
        name: "paths-strokes",
        doc,
        ticks: vec![0, dur / 2],
    }
}

/// Curved paths from SVG path data: the shapes whose flattening and arc
/// conversion could differ between native and wasm.
///
/// Covers cubic `C`/`S`, quadratic `Q`/`T`, a circle drawn as two `A`
/// commands (gradient filled, so the shader follows an arc-built silhouette),
/// and an arc with a non-zero x-axis rotation and unequal radii, rotated as
/// an element so the pivot comes from the curve's tight bounds.
fn curves() -> CorpusDoc {
    let dur = ms(600);
    let mut doc = Document::new(W, H);
    let scene = Scene::new("s", dur)
        .with_element(
            Element::path_data("M 12 90 C 40 10 90 10 110 60 S 170 110 200 40")
                .with_stroke("#FF9900", 6.5)
                .with_cap(Cap::Round)
                .with_join(Join::Round),
        )
        .with_element(
            Element::path_data("M 20 180 Q 60 120 100 160 T 180 150 L 150 192 Z")
                .with_path_fill("#4ECDC4")
                .with_stroke("#F2F5F7", 3.0)
                .with_join(Join::Miter),
        )
        .with_element(
            Element::path_data("M 210 140 A 40 40 0 1 0 290 140 A 40 40 0 1 0 210 140 Z")
                .with_path_fill(Gradient::linear(
                    [0.0, 0.0],
                    [1.0, 1.0],
                    vec![Stop::new(0.0, "#FF9900"), Stop::new(1.0, "#C77DFF")],
                )),
        )
        // Unequal radii and a tilted axis: the awkward-angle arc.
        .with_element(
            Element::path_data("M 215 30 A 37 23 31 1 0 262 60")
                .with_stroke("#F2F5F7", 2.5)
                .with_cap(Cap::Round)
                .with_rotation(17.0),
        );
    doc.push_scene(scene);
    CorpusDoc {
        name: "curves",
        doc,
        ticks: vec![0, dur / 2],
    }
}

/// Curves that reach tiny-skia's platform-trig paths, which `curves` does not.
///
/// Stroking a cubic whose control points are collinear calls `acos`/`cos`/
/// `powf`, and clipping a cubic that crosses the canvas edge calls f64
/// `acos`/`cos`. Both differ by platform libm, so native and wasm only agree
/// here if the parser's output keeps clear of them or tiny-skia's results
/// happen to match — the parity gate has to see these shapes to say which.
fn curves_edge() -> CorpusDoc {
    let mut doc = Document::new(W, H);
    let scene = Scene::new("s", ms(600))
        // A `T` with no preceding `Q` degenerates to an exactly collinear cubic.
        .with_element(
            Element::path_data("M 10 10 T 100 100")
                .with_stroke("#FF9900", 5.0)
                .with_cap(Cap::Round),
        )
        // A cubic written as a straight line.
        .with_element(
            Element::path_data("M 20 150 C 40 150 60 150 80 150")
                .with_stroke("#C77DFF", 6.0)
                .with_cap(Cap::Butt),
        )
        // Leaves the canvas past the right and bottom edges, filled and stroked.
        .with_element(
            Element::path_data("M 230 110 C 360 90 360 260 240 230 Z")
                .with_path_fill("#4ECDC4")
                .with_stroke("#F2F5F7", 4.0)
                .with_join(Join::Round),
        )
        // The same crossing on the open-arc side: an arc past the top-left.
        .with_element(
            Element::path_data("M 90 60 A 70 70 0 1 1 20 -10")
                .with_stroke("#FF4040", 3.0)
                .with_cap(Cap::Round),
        );
    doc.push_scene(scene);
    CorpusDoc {
        name: "curves-edge",
        doc,
        ticks: vec![0],
    }
}

/// Collinear cubics whose control points are unevenly spaced.
///
/// `curves_edge`'s collinear shapes have evenly spaced control points, so
/// the stroker's `C = d + 3(b - c) - a` is zero and it takes the trig-free
/// branch. A non-zero `C` on at least one axis is what sends it through
/// `find_cubic_max_curvature` and its platform `acos`/`cos`/`powf`.
fn curves_collinear() -> CorpusDoc {
    let mut doc = Document::new(W, H);
    let scene = Scene::new("s", ms(600))
        // C = (-140, -140).
        .with_element(
            Element::path_data("M 20 20 C 30 30 110 110 120 120")
                .with_stroke("#FF9900", 4.0)
                .with_cap(Cap::Round),
        )
        // C = (-210, 0).
        .with_element(
            Element::path_data("M 20 160 C 30 160 150 160 170 160")
                .with_stroke("#C77DFF", 4.0)
                .with_cap(Cap::Round),
        )
        // Control points overshoot the endpoints, so the curve doubles back: C = (280, 0).
        .with_element(
            Element::path_data("M 200 40 C 260 40 180 40 240 40")
                .with_stroke("#4ECDC4", 4.0)
                .with_cap(Cap::Round),
        );
    doc.push_scene(scene);
    CorpusDoc {
        name: "curves-collinear",
        doc,
        ticks: vec![0],
    }
}

/// `draw` partway through its range: the trimmed-stroke geometry whose
/// cut points native and wasm must agree on.
///
/// A curved arrow (the cut lands inside a cubic), a filled closed square
/// (the fill must stay whole while the outline grows, and a partial contour
/// loses its close), and a two-subpath shape (the cut lands in the second
/// contour, so the first must be complete by then).
fn draw_on() -> CorpusDoc {
    let draw_in = || {
        Track::new(
            Prop::Draw,
            vec![
                Key::num(0, 0.0),
                Key::num(ms(1000), 1.0).with_ease(Ease::InOutCubic),
            ],
        )
    };
    let mut doc = Document::new(W, H);
    let scene = Scene::new("s", ms(1200))
        .with_element(
            Element::path_data("M 14 150 C 50 60 120 60 170 110 L 150 100 M 170 110 L 160 132")
                .with_stroke("#FF9900", 5.5)
                .with_cap(Cap::Round)
                .with_join(Join::Round)
                .with_animation(draw_in()),
        )
        .with_element(
            Element::path(vec![[20.0, 30.0], [90.0, 24.0], [96.0, 80.0], [26.0, 86.0]])
                .with_closed(true)
                .with_path_fill("#4ECDC4")
                .with_stroke("#F2F5F7", 3.0)
                .with_animation(draw_in()),
        )
        .with_element(
            Element::path_data("M 200 30 L 300 30 M 200 70 C 230 120 270 20 300 90")
                .with_stroke("#C77DFF", 4.0)
                .with_cap(Cap::Butt)
                .with_animation(draw_in()),
        );
    doc.push_scene(scene);
    CorpusDoc {
        name: "draw-on",
        doc,
        ticks: vec![0, ms(250), ms(500), ms(1000)],
    }
}

/// Line-only paths cast the shadow of their stroked outline, which tiny-skia's
/// stroker builds and the blur then spreads. The outline is new geometry the
/// stroke itself never rasterised as a fill, so it needs its own parity
/// evidence: a blurred shadow following a curve mid-draw, and a hard shadow
/// under a rotated polyline with round joins.
fn shadow_stroke() -> CorpusDoc {
    let mut doc = Document::new(W, H);
    let scene = Scene::new("s", ms(1200))
        .with_element(
            Element::path_data("M 16 160 C 60 40 150 40 200 120 S 280 170 300 60")
                .with_stroke("#FF9900", 5.0)
                .with_cap(Cap::Round)
                .with_shadow(Shadow::new("#0D141999", 6, 3.0, 8.0))
                .with_animation(Track::new(
                    Prop::Draw,
                    vec![
                        Key::num(0, 0.0),
                        Key::num(ms(1000), 1.0).with_ease(Ease::InOutCubic),
                    ],
                )),
        )
        .with_element(
            Element::path(vec![[30.0, 30.0], [110.0, 42.0], [150.0, 20.0]])
                .with_stroke("#4ECDC4", 3.5)
                .with_join(Join::Round)
                .with_shadow(Shadow::new("#0D1419", 0, 4.0, 5.0))
                .with_rotation(9.0),
        );
    doc.push_scene(scene);
    CorpusDoc {
        name: "shadow-stroke",
        doc,
        ticks: vec![ms(400), ms(1000)],
    }
}

/// grad.png (rotated + scaled, static transform) and photo.jpg (opacity
/// ramp, animated) side by side.
fn image_transform() -> CorpusDoc {
    let dur = ms(600);
    let mut doc = Document::new(W, H);
    doc.add_asset("grad", Asset::image("grad.png"));
    doc.add_asset("photo", Asset::image("photo.jpg"));
    let scene = Scene::new("s", dur)
        .with_element(
            Element::image("grad", [20.0, 20.0, 120.0, 80.0])
                .with_rotation(15.0)
                .with_scale(1.2),
        )
        .with_element(
            Element::image("photo", [160.0, 40.0, 120.0, 120.0]).with_animation(Track::new(
                Prop::Opacity,
                vec![Key::num(0, 0.0), Key::num(dur, 1.0)],
            )),
        );
    doc.push_scene(scene);
    CorpusDoc {
        name: "image-transform",
        doc,
        ticks: vec![0, dur / 2, dur - 1],
    }
}

/// One Inter paragraph, wrapped (`maxW`), in all three aligns, plus a
/// JetBrains Mono line.
fn text_wrap() -> CorpusDoc {
    let dur = ms(500);
    let mut doc = Document::new(W, H);
    doc.add_asset("inter", Asset::font("kineto:inter"));
    doc.add_asset("mono", Asset::font("kineto:jetbrains-mono"));
    let para = "Deterministic video, twice.";
    let scene = Scene::new("s", dur)
        .with_element(
            Element::text(para, "inter", 18.0, "#FFFFFF", [10.0, 10.0])
                .with_max_w(150.0)
                .with_align(Align::Left),
        )
        .with_element(
            Element::text(para, "inter", 18.0, "#FFFFFF", [10.0, 70.0])
                .with_max_w(150.0)
                .with_align(Align::Center),
        )
        .with_element(
            Element::text(para, "inter", 18.0, "#FFFFFF", [10.0, 130.0])
                .with_max_w(150.0)
                .with_align(Align::Right),
        )
        .with_element(Element::text(
            "JetBrains Mono line",
            "mono",
            16.0,
            "#D4D4D4",
            [10.0, 178.0],
        ));
    doc.push_scene(scene);
    CorpusDoc {
        name: "text-wrap",
        doc,
        ticks: vec![0, dur / 2],
    }
}

/// Nested groups: outer group's own opacity composites the whole subtree as
/// one isolated unit; inner group is rotated.
fn groups_nested() -> CorpusDoc {
    let dur = ms(500);
    let mut doc = Document::new(W, H);
    let inner = Element::group(
        [40.0, 40.0],
        vec![
            Element::rect([0.0, 0.0, 60.0, 60.0], "#FF0000").with_opacity(0.6),
            Element::rect([20.0, 20.0, 60.0, 60.0], "#00FF00").with_opacity(0.6),
        ],
    )
    .with_rotation(20.0);
    let outer = Element::group([30.0, 30.0], vec![inner]).with_opacity(0.7);
    let scene = Scene::new("s", dur).with_element(outer);
    doc.push_scene(scene);
    CorpusDoc {
        name: "groups-nested",
        doc,
        ticks: vec![0, dur / 2],
    }
}

/// Two image scenes, 300ms crossfade. Ticks straddle the incoming window
/// `[start[b], start[b]+overlap)` = `[300ms, 600ms)`: before, window start,
/// exact mid, window end, after.
fn crossfade_doc() -> CorpusDoc {
    let dur = ms(600);
    let overlap = ms(300);
    let mut doc = Document::new(W, H);
    doc.add_asset("grad", Asset::image("grad.png"));
    doc.add_asset("photo", Asset::image("photo.jpg"));
    doc.push_scene(
        Scene::new("a", dur).with_element(Element::image("grad", [0.0, 0.0, W as f64, H as f64])),
    );
    doc.push_scene(
        Scene::new("b", dur)
            .with_transition(Transition::crossfade(overlap))
            .with_element(Element::image("photo", [0.0, 0.0, W as f64, H as f64])),
    );
    CorpusDoc {
        name: "crossfade",
        doc,
        ticks: vec![ms(250), ms(300), ms(450), ms(600), ms(650)],
    }
}

/// The spec §3.6 example, reshaped onto the 320x200 corpus canvas and
/// corpus assets: image + caption bar + fading mono caption, crossfading
/// into a second image scene.
fn kitchen_sink() -> CorpusDoc {
    let dur = ms(900);
    let overlap = ms(150);
    let mut doc = Document::new(W, H);
    doc.add_asset("grad", Asset::image("grad.png"));
    doc.add_asset("photo", Asset::image("photo.jpg"));
    doc.add_asset("mono", Asset::font("kineto:jetbrains-mono"));
    doc.push_scene(
        Scene::new("step-1", dur)
            .with_element(Element::image("grad", [0.0, 0.0, W as f64, H as f64]))
            .with_element(Element::rect([0.0, 150.0, W as f64, 50.0], "#0A0A0AE6"))
            .with_element(
                Element::text(
                    "Landing on the page. Big cookie banner, ominous.",
                    "mono",
                    16.0,
                    "#D4D4D4",
                    [10.0, 160.0],
                )
                .with_max_w(300.0)
                .with_animation(Track::new(
                    Prop::Opacity,
                    vec![
                        Key::num(0, 0.0),
                        Key::num(ms(180), 1.0).with_ease(Ease::OutCubic),
                    ],
                )),
            ),
    );
    doc.push_scene(
        Scene::new("step-2", dur)
            .with_transition(Transition::crossfade(overlap))
            .with_element(Element::image("photo", [0.0, 0.0, W as f64, H as f64])),
    );
    CorpusDoc {
        name: "kitchen-sink",
        doc,
        ticks: vec![ms(50), ms(800), ms(1400)],
    }
}

/// Decode/load every asset `doc` references, for native rendering: reserved
/// `kineto:*` srcs resolve via `resolve_reserved_src`; everything else is
/// read from `testdata/assets/<src>`. Native-and-`bundled-fonts`-only — it
/// calls `resolve_reserved_src`, which only exists with that feature on, and
/// touches `std::fs`, which wasm32 doesn't have. The wasm harness (Task 16)
/// supplies asset bytes its own way, so this is not part of the
/// wasm-compiled surface (`crates/wasm` depends on `kineto-core` with
/// `default-features = false`, i.e. `bundled-fonts` off, even for its own
/// native unit tests — see `crates/wasm`'s `Cargo.toml`).
#[cfg(all(not(target_arch = "wasm32"), feature = "bundled-fonts"))]
pub fn corpus_load_assets(doc: &Document) -> AssetStore {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/assets");
    let mut assets = AssetStore::new();
    for (id, asset) in &doc.assets {
        let src = match asset {
            Asset::Image { src } => src,
            Asset::Font { src } => src,
        };
        let bytes = resolve_reserved_src(src)
            .map(|b| b.to_vec())
            .unwrap_or_else(|| {
                std::fs::read(dir.join(src))
                    .unwrap_or_else(|e| panic!("corpus_load_assets: failed reading '{src}': {e}"))
            });
        assets.add_bytes(id, bytes);
    }
    assets
        .prepare(doc)
        .expect("corpus_load_assets: AssetStore::prepare failed");
    assets
}
