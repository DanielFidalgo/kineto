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
    ms, Align, Asset, Cap, Document, Ease, Element, Gradient, Join, Key, Prop, Scene, Stop, Track,
    Transition,
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
        gradients(),
        radius_easings(),
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
